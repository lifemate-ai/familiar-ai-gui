import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";

interface Props {
  code: string;
  language?: string;
}

function detectLanguage(code: string): string {
  if (/^(fn |pub fn |use |mod |impl |struct |enum )/.test(code)) return "rust";
  if (/^(import |export |const |let |var |function |class )/.test(code)) return "typescript";
  if (/^(def |import |from |class |if __name__)/.test(code)) return "python";
  if (/^\$|^#!\/bin\//.test(code)) return "bash";
  if (/^\{[\s\S]*\}$/.test(code.trim())) return "json";
  return "text";
}

export function CodeBlock({ code, language }: Props) {
  const lang = language ?? detectLanguage(code);
  return (
    <SyntaxHighlighter
      language={lang}
      style={oneDark}
      customStyle={{
        margin: "0.4rem 0",
        borderRadius: "8px",
        fontSize: "0.85rem",
        padding: "0.8rem 1rem",
      }}
      wrapLongLines
    >
      {code}
    </SyntaxHighlighter>
  );
}
