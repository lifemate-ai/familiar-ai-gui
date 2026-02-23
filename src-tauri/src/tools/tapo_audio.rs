/// Tapo camera audio backchannel — plays audio through the camera's speaker.
///
/// Protocol (reverse-engineered from go2rtc pkg/tapo):
///   1. Raw TCP POST to http://host:8800/stream → 401
///   2. HTTP Digest auth → 200 (connection stays open for bidirectional streaming)
///   3. Send session negotiation JSON as multipart frame → read session_id
///   4. Stream G.711 PCMA audio wrapped in MPEGTS as multipart frames
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

// ── MPEGTS constants ─────────────────────────────────────────────────────────

const SYNC_BYTE: u8 = 0x47;
const TS_PACKET_SIZE: usize = 188;
const PAT_PID: u16 = 0;
const PMT_PID: u16 = 0x1000;
const AUDIO_PID: u16 = 0x100;
/// go2rtc StreamTypePCMATapo = 0x90
const STREAM_TYPE_PCMA_TAPO: u8 = 0x90;
const AUDIO_STREAM_ID: u8 = 0xC0;

// ── CRC-32/MPEG ───────────────────────────────────────────────────────────────

fn crc32_mpeg(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        let mut b = byte as u32;
        for _ in 0..8 {
            if (crc ^ (b << 24)) & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ 0x04C1_1DB7;
            } else {
                crc <<= 1;
            }
            b <<= 1;
        }
    }
    crc
}

// ── MPEGTS TS packet builder ──────────────────────────────────────────────────

/// Pad a payload to a full 188-byte TS packet.
/// `pid`: 13-bit PID. `pusi`: Payload Unit Start Indicator. `cont`: continuity counter (4-bit).
fn ts_packet(pid: u16, pusi: bool, cont: u8, payload: &[u8]) -> [u8; TS_PACKET_SIZE] {
    let mut pkt = [0xFFu8; TS_PACKET_SIZE];
    pkt[0] = SYNC_BYTE;
    let flags_pid = (if pusi { 0x4000u16 } else { 0 }) | (pid & 0x1FFF);
    pkt[1] = (flags_pid >> 8) as u8;
    pkt[2] = (flags_pid & 0xFF) as u8;
    // adaptation_field_control=01 (payload only), continuity
    pkt[3] = 0x10 | (cont & 0x0F);

    let max_payload = TS_PACKET_SIZE - 4;
    if payload.len() < max_payload {
        // Adaptation field with stuffing
        let stuff_len = max_payload - payload.len() - 1; // -1 for adaptation_field_length byte
        pkt[3] = 0x30 | (cont & 0x0F); // adaptation+payload
        pkt[4] = stuff_len as u8;
        if stuff_len > 0 {
            pkt[5] = 0x00; // no flags
            // bytes 6..4+stuff_len are already 0xFF
        }
        let start = 4 + 1 + stuff_len;
        pkt[start..start + payload.len()].copy_from_slice(payload);
    } else {
        pkt[4..4 + payload.len().min(max_payload)].copy_from_slice(&payload[..max_payload.min(payload.len())]);
    }
    pkt
}

/// Build PAT + PMT TS packets (MPEGTS header = 376 bytes).
pub fn build_mpegts_header() -> Vec<u8> {
    // ── PAT ──
    let mut pat_section = Vec::with_capacity(17);
    pat_section.push(0x00); // pointer field
    pat_section.push(0x00); // table_id = PAT
    // section_syntax=1, reserved=11, section_length = 13
    pat_section.push(0xB0);
    pat_section.push(0x0D);
    pat_section.extend_from_slice(&[0x00, 0x01]); // transport_stream_id = 1
    pat_section.push(0xC1); // reserved=11, version=0, current_next=1
    pat_section.push(0x00); // section_number
    pat_section.push(0x00); // last_section_number
    // program 1 → PMT PID 0x1000: reserved(3) + pid(13) = 0b111_1_0000_0000_0000 = 0xF0 0x00
    pat_section.extend_from_slice(&[0x00, 0x01]); // program_number
    pat_section.push(0xE0 | ((PMT_PID >> 8) & 0x1F) as u8);
    pat_section.push((PMT_PID & 0xFF) as u8);
    // CRC32 over bytes after pointer field
    let crc = crc32_mpeg(&pat_section[1..]);
    pat_section.extend_from_slice(&crc.to_be_bytes());
    let pat_pkt = ts_packet(PAT_PID, true, 0, &pat_section);

    // ── PMT ──
    // section_length = 9 (base) + 5 (one stream entry) + 4 (CRC) = 18 = 0x12
    let mut pmt_section = Vec::with_capacity(27);
    pmt_section.push(0x00); // pointer field
    pmt_section.push(0x02); // table_id = PMT
    pmt_section.push(0xB0);
    pmt_section.push(0x12); // section_length = 18
    pmt_section.extend_from_slice(&[0x00, 0x01]); // program_number
    pmt_section.push(0xC1);
    pmt_section.push(0x00);
    pmt_section.push(0x00);
    // PCR_PID = 0x1FFF (none): reserved(3)+pid(13) = 0xFF 0xFF
    pmt_section.extend_from_slice(&[0xFF, 0xFF]);
    // program_info_length = 0: reserved(4)+length(12) = 0xF0 0x00
    pmt_section.extend_from_slice(&[0xF0, 0x00]);
    // stream entry: stream_type + pid + ES_info_length
    pmt_section.push(STREAM_TYPE_PCMA_TAPO);
    pmt_section.push(0xE0 | ((AUDIO_PID >> 8) & 0x1F) as u8);
    pmt_section.push((AUDIO_PID & 0xFF) as u8);
    pmt_section.extend_from_slice(&[0xF0, 0x00]); // ES_info_length = 0
    let crc = crc32_mpeg(&pmt_section[1..]);
    pmt_section.extend_from_slice(&crc.to_be_bytes());
    let pmt_pkt = ts_packet(PMT_PID, true, 0, &pmt_section);

    let mut out = Vec::with_capacity(TS_PACKET_SIZE * 2);
    out.extend_from_slice(&pat_pkt);
    out.extend_from_slice(&pmt_pkt);
    out
}

/// Encode PTS as 5 bytes (MPEG TS format, PTS-only).
fn encode_pts(pts: u64) -> [u8; 5] {
    [
        0x21 | ((pts >> 29) as u8 & 0x0E),
        (pts >> 22) as u8,
        0x01 | ((pts >> 14) as u8 & 0xFE),
        (pts >> 7) as u8,
        0x01 | ((pts << 1) as u8 & 0xFE),
    ]
}

/// Wrap raw PCMA payload into one or more 188-byte TS packets with PES header.
pub fn pcma_to_ts_packets(payload: &[u8], pts: u64, continuity: &mut u8) -> Vec<u8> {
    // Build PES header (14 bytes)
    let pes_header_data_len: u16 = 5; // PTS only
    let pes_packet_len: u16 = 3 + pes_header_data_len + payload.len() as u16;
    // 3 = flags(1) + flags(1) + header_data_length(1)
    let mut pes = Vec::with_capacity(14 + payload.len());
    pes.extend_from_slice(&[0x00, 0x00, 0x01, AUDIO_STREAM_ID]);
    pes.extend_from_slice(&pes_packet_len.to_be_bytes());
    pes.push(0x80); // flags1: marker=10, no scrambling
    pes.push(0x80); // flags2: PTS_DTS_flags=10 (PTS only)
    pes.push(pes_header_data_len as u8);
    pes.extend_from_slice(&encode_pts(pts));
    pes.extend_from_slice(payload);

    // Split PES into 184-byte TS payloads
    let mut out = Vec::new();
    let mut first = true;
    let mut offset = 0;
    while offset < pes.len() {
        let chunk_len = (pes.len() - offset).min(TS_PACKET_SIZE - 4);
        let chunk = &pes[offset..offset + chunk_len];
        let pkt = ts_packet(AUDIO_PID, first, *continuity, chunk);
        *continuity = (*continuity + 1) & 0x0F;
        out.extend_from_slice(&pkt);
        offset += chunk_len;
        first = false;
    }
    out
}

// ── HTTP Digest auth ──────────────────────────────────────────────────────────

fn hex_md5(parts: &[&str]) -> String {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            h.update(b":");
        }
        h.update(p.as_bytes());
    }
    hex::encode(h.finalize())
}

fn between<'a>(s: &'a str, open: &str, close: &str) -> &'a str {
    let start = s.find(open).map(|i| i + open.len()).unwrap_or(0);
    let end = s[start..].find(close).map(|i| i + start).unwrap_or(s.len());
    &s[start..end]
}

/// Resolve Tapo-specific credentials from the WWW-Authenticate challenge.
///
/// When the camera returns `encrypt_type="3"`, it expects:
///   username = "admin"
///   password = SHA256(cloud_password) as 64 uppercase hex chars
/// When `encrypt_type` is present but not "3":
///   password = MD5(cloud_password) as 32 uppercase hex chars
///
/// If `password` is empty, treats `username` as the cloud password
/// (go2rtc `tapo://cloud_pass@ip` style).
fn tapo_resolve_credentials(username: &str, password: &str, www_auth: &str) -> (String, String) {
    if !www_auth.contains("encrypt_type") {
        return (username.to_string(), password.to_string());
    }
    let cloud = if !password.is_empty() { password } else { username };
    let hashed = if www_auth.contains(r#"encrypt_type="3""#) {
        use sha2::{Digest as _, Sha256};
        hex::encode(Sha256::digest(cloud.as_bytes())).to_uppercase()
    } else {
        use md5::{Digest as _, Md5};
        hex::encode(Md5::digest(cloud.as_bytes())).to_uppercase()
    };
    ("admin".to_string(), hashed)
}

fn build_digest_auth(
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    auth_header: &str,
) -> String {
    let realm = between(auth_header, "realm=\"", "\"");
    let nonce = between(auth_header, "nonce=\"", "\"");
    let qop = between(auth_header, "qop=\"", "\"");
    let opaque = between(auth_header, "opaque=\"", "\"");

    let ha1 = hex_md5(&[username, realm, password]);
    let ha2 = hex_md5(&[method, uri]);
    let nc = "00000001";
    let cnonce = format!("{:032x}", rand_u128());
    let response = hex_md5(&[&ha1, nonce, nc, &cnonce, qop, &ha2]);

    let mut hdr = format!(
        r#"Digest username="{username}", realm="{realm}", nonce="{nonce}", uri="{uri}", qop={qop}, nc={nc}, cnonce="{cnonce}", response="{response}""#
    );
    if !opaque.is_empty() {
        hdr.push_str(&format!(r#", opaque="{opaque}", algorithm=MD5"#));
    }
    hdr
}

fn rand_u128() -> u128 {
    // Simple PRNG seeded from system time (good enough for cnonce)
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u128;
    t.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

// ── Raw HTTP helpers ──────────────────────────────────────────────────────────

async fn read_http_response_headers(
    reader: &mut BufReader<impl AsyncReadExt + Unpin>,
) -> Result<(u16, HashMap<String, String>)> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if trimmed.is_empty() {
            break;
        }
        lines.push(trimmed);
    }
    if lines.is_empty() {
        bail!("empty HTTP response");
    }
    let status: u16 = lines[0]
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let mut headers = HashMap::new();
    for line in &lines[1..] {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    Ok((status, headers))
}

fn http_post_request(host: &str, port: u16, auth_header: Option<&str>) -> String {
    let uri = "/stream";
    let mut req = format!(
        "POST {uri} HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Content-Type: multipart/mixed; boundary=--client-stream-boundary--\r\n\
         Content-Length: 0\r\n"
    );
    if let Some(auth) = auth_header {
        req.push_str(&format!("Authorization: {auth}\r\n"));
    }
    req.push_str("\r\n");
    req
}

// ── Multipart frame builder ───────────────────────────────────────────────────

fn multipart_json_frame(body: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"----client-stream-boundary--\r\n");
    buf.extend_from_slice(b"Content-Type: application/json\r\n");
    buf.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    buf.extend_from_slice(body);
    buf.extend_from_slice(b"\r\n");
    buf
}

fn multipart_audio_frame(session_id: &str, mpegts: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"----client-stream-boundary--\r\n");
    buf.extend_from_slice(b"Content-Type: audio/mp2t\r\n");
    buf.extend_from_slice(b"X-If-Encrypt: 0\r\n");
    buf.extend_from_slice(format!("X-Session-Id: {session_id}\r\n").as_bytes());
    buf.extend_from_slice(format!("Content-Length: {}\r\n\r\n", mpegts.len()).as_bytes());
    buf.extend_from_slice(mpegts);
    buf
}

// ── MP3 → raw PCMA via ffmpeg ─────────────────────────────────────────────────

async fn mp3_to_pcma(mp3_bytes: Vec<u8>) -> Result<Vec<u8>> {
    // ffmpeg: stdin=mp3 → stdout=raw alaw 8kHz mono
    let mut child = tokio::process::Command::new("ffmpeg")
        .args([
            "-loglevel", "error",
            "-i", "pipe:0",
            "-f", "alaw",
            "-ar", "8000",
            "-ac", "1",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    // Write MP3 to stdin in a background task
    let mut stdin = child.stdin.take().unwrap();
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&mp3_bytes).await;
        // drop closes stdin, signaling EOF to ffmpeg
    });

    // Read PCM from stdout
    let mut pcma = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_end(&mut pcma).await?;
    }

    writer.await.ok();
    child.wait().await?;
    Ok(pcma)
}

// ── Session negotiation (reads multipart JSON response, extracts session_id) ──

async fn read_session_id(
    reader: &mut BufReader<impl AsyncReadExt + Unpin>,
) -> Result<String> {
    // The server sends multipart data with boundary "--device-stream-boundary--".
    // We scan frames until we find one whose JSON contains params.session_id.
    loop {
        // ── Find boundary line ──
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                bail!("connection closed before session_id");
            }
            if line.contains("device-stream-boundary") {
                break;
            }
        }

        // ── Read frame headers ──
        let mut body_len: usize = 0;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                bail!("connection closed reading frame headers");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break; // end of headers — body follows immediately
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    body_len = v.trim().parse().unwrap_or(0);
                }
            }
        }

        // ── Read frame body ──
        if body_len == 0 {
            continue; // no body, skip to next frame
        }
        let mut body = vec![0u8; body_len];
        reader.read_exact(&mut body).await?;

        // ── Try to extract session_id ──
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) {
            if let Some(sid) = json["params"]["session_id"].as_str() {
                if !sid.is_empty() {
                    return Ok(sid.to_string());
                }
            }
        }
        // Frame didn't have session_id (e.g. status frame), keep scanning
    }
}

// ── TapoAudio public API ───────────────────────────────────────────────────────

pub struct TapoAudio {
    host: String,
    username: String,
    password: String,
}

impl TapoAudio {
    pub fn new(host: impl Into<String>, username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            username: username.into(),
            password: password.into(),
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.host.is_empty()
    }

    /// Play MP3 audio through the Tapo camera speaker.
    pub async fn play(&self, mp3_bytes: Vec<u8>) -> Result<()> {
        // Convert MP3 → raw A-law 8kHz
        let pcma = mp3_to_pcma(mp3_bytes).await?;
        if pcma.is_empty() {
            bail!("ffmpeg produced no output — is ffmpeg installed?");
        }

        // Connect & authenticate
        let (mut writer, session_id) = self.connect_and_authenticate().await?;

        // Send MPEGTS header (PAT + PMT)
        let header = build_mpegts_header();
        let frame = multipart_audio_frame(&session_id, &header);
        writer.write_all(&frame).await?;

        // Send audio in 20ms chunks (160 samples = 160 bytes of PCMA at 8kHz)
        let chunk_size = 160;
        let mut continuity: u8 = 0;
        let mut pts: u64 = 0;
        // PTS increment: 160 samples * (90000 / 8000) = 1800 ticks per chunk
        let pts_increment: u64 = 1800;

        for chunk in pcma.chunks(chunk_size) {
            let ts_data = pcma_to_ts_packets(chunk, pts, &mut continuity);
            let frame = multipart_audio_frame(&session_id, &ts_data);
            writer.write_all(&frame).await?;
            pts = pts.wrapping_add(pts_increment);
        }

        writer.flush().await.ok();
        Ok(())
    }

    async fn connect_and_authenticate(&self) -> Result<(impl AsyncWriteExt + Unpin, String)> {
        let addr = format!("{}:8800", self.host);
        let stream = TcpStream::connect(&addr).await?;

        // Split for reading headers while keeping the write half
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // ── Step 1: initial POST → expect 401 ──
        let req1 = http_post_request(&self.host, 8800, None);
        write_half.write_all(req1.as_bytes()).await?;

        let (status1, headers1) = read_http_response_headers(&mut reader).await?;
        if status1 != 401 {
            bail!("expected 401, got {status1}");
        }
        let www_auth = headers1
            .get("www-authenticate")
            .cloned()
            .unwrap_or_default();
        if !www_auth.to_lowercase().starts_with("digest") {
            bail!("expected Digest auth, got: {www_auth}");
        }

        // ── Step 2: Tapo cloud credential resolution + authenticated POST ──
        let (eff_user, eff_pass) =
            tapo_resolve_credentials(&self.username, &self.password, &www_auth);
        let auth = build_digest_auth(&eff_user, &eff_pass, "POST", "/stream", &www_auth);
        let req2 = http_post_request(&self.host, 8800, Some(&auth));
        write_half.write_all(req2.as_bytes()).await?;

        let (status2, _) = read_http_response_headers(&mut reader).await?;
        if status2 != 200 {
            bail!("authentication failed, got {status2}");
        }

        // ── Step 3: session negotiation ──
        let talk_req = br#"{"params":{"talk":{"mode":"aec"},"method":"get"},"seq":3,"type":"request"}"#;
        let frame = multipart_json_frame(talk_req);
        write_half.write_all(&frame).await?;

        // Read session_id from response
        let session_id = read_session_id(&mut reader).await?;

        // Drain the reader in background (camera keeps sending video/metadata)
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            loop {
                if reader.read(&mut buf).await.unwrap_or(0) == 0 {
                    break;
                }
            }
        });

        Ok((write_half, session_id))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_mpeg_known() {
        // Empty data CRC known value for MPEG CRC32
        let crc = crc32_mpeg(&[]);
        assert_eq!(crc, 0xFFFF_FFFF);
    }

    #[test]
    fn test_ts_packet_size() {
        let pkt = ts_packet(0, true, 0, &[0x00u8; 4]);
        assert_eq!(pkt.len(), 188);
        assert_eq!(pkt[0], 0x47);
    }

    #[test]
    fn test_mpegts_header_size() {
        let hdr = build_mpegts_header();
        assert_eq!(hdr.len(), 376);
        // Both packets start with sync byte
        assert_eq!(hdr[0], 0x47);
        assert_eq!(hdr[188], 0x47);
    }

    #[test]
    fn test_mpegts_header_pat_pid() {
        let hdr = build_mpegts_header();
        // PAT PID = 0: bytes 1-2 should be 0x40 0x00 (PUSI=1, PID=0)
        assert_eq!(hdr[1] & 0x1F, 0x00);
        assert_eq!(hdr[2], 0x00);
    }

    #[test]
    fn test_mpegts_header_pmt_pid() {
        let hdr = build_mpegts_header();
        // PMT PID = 0x1000: bytes 189-190
        let pid_high = (hdr[189] & 0x1F) as u16;
        let pid_low = hdr[190] as u16;
        let pid = (pid_high << 8) | pid_low;
        assert_eq!(pid, PMT_PID);
    }

    #[test]
    fn test_pmt_has_pcma_tapo_stream_type() {
        let hdr = build_mpegts_header();
        // PMT payload contains STREAM_TYPE_PCMA_TAPO = 0x90
        // After 4-byte TS header + pointer byte, scan for 0x90
        let pmt = &hdr[188..];
        assert!(pmt[4..].contains(&STREAM_TYPE_PCMA_TAPO),
            "PMT should contain stream type 0x90");
    }

    #[test]
    fn test_encode_pts_zero() {
        let pts = encode_pts(0);
        assert_eq!(pts[0], 0x21); // 0b0010_0001
        assert_eq!(pts[2] & 0x01, 0x01); // marker bit
        assert_eq!(pts[4] & 0x01, 0x01); // marker bit
    }

    #[test]
    fn test_encode_pts_nonzero() {
        // PTS = 90000 (1 second at 90kHz)
        let pts = encode_pts(90000);
        // Verify marker bits present
        assert_eq!(pts[0] & 0x01, 0x01);
        assert_eq!(pts[2] & 0x01, 0x01);
        assert_eq!(pts[4] & 0x01, 0x01);
        // Top nibble should be 0b0010
        assert_eq!(pts[0] >> 4, 0x02);
    }

    #[test]
    fn test_pcma_to_ts_packets_not_empty() {
        let payload = vec![0u8; 160];
        let mut cont = 0u8;
        let pkts = pcma_to_ts_packets(&payload, 0, &mut cont);
        assert!(!pkts.is_empty());
        assert_eq!(pkts.len() % 188, 0);
        assert_eq!(pkts[0], 0x47); // sync byte
    }

    #[test]
    fn test_continuity_counter_increments() {
        let payload = vec![0u8; 160];
        let mut cont = 0u8;
        pcma_to_ts_packets(&payload, 0, &mut cont);
        assert!(cont > 0, "continuity counter should have incremented");
    }

    #[test]
    fn test_multipart_audio_frame_format() {
        let frame = multipart_audio_frame("test-session", b"audio");
        let s = String::from_utf8_lossy(&frame);
        assert!(s.contains("----client-stream-boundary--"));
        assert!(s.contains("audio/mp2t"));
        assert!(s.contains("X-If-Encrypt: 0"));
        assert!(s.contains("X-Session-Id: test-session"));
        assert!(s.contains("Content-Length: 5"));
    }

    #[test]
    fn test_multipart_json_frame_format() {
        let body = b"{}";
        let frame = multipart_json_frame(body);
        let s = String::from_utf8_lossy(&frame);
        assert!(s.contains("----client-stream-boundary--"));
        assert!(s.contains("application/json"));
        assert!(s.contains("Content-Length: 2"));
    }

    #[test]
    fn test_hex_md5() {
        // MD5("") = d41d8cd98f00b204e9800998ecf8427e
        let result = hex_md5(&[""]);
        assert_eq!(result, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_hex_md5_parts() {
        // MD5("admin:realm") should be consistent
        let r1 = hex_md5(&["admin", "realm"]);
        let r2 = hex_md5(&["admin", "realm"]);
        assert_eq!(r1, r2);
        assert_eq!(r1.len(), 32);
    }
}
