// components/InstructionsPanel.tsx
//
// Renders the markdown instructions for the current step.
// Images in the markdown use relative paths like assets/image.png —
// we rewrite them to use Tauri's asset protocol so they load from
// the extracted temp directory.
//
// Requires: npm install react-markdown remark-gfm

import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { convertFileSrc } from "@tauri-apps/api/core";

type Props = {
  markdown: string;
  tempDir: string; // absolute path to the extracted profile temp directory
};

export function InstructionsPanel({ markdown, tempDir }: Props) {
  // Rewrites relative asset paths in markdown image tags to use
  // Tauri's asset protocol, which can serve local files to the WebView.
  // e.g. "assets/import_profile.png" →
  //      "tauri://localhost/path/to/temp/assets/import_profile.png"
  function resolveImageSrc(src: string | undefined): string {
    if (!src) return "";
    if (src.startsWith("http://") || src.startsWith("https://")) return src;
    const absolutePath = `${tempDir}/${src}`.replace(/\/+/g, "/");
    return convertFileSrc(absolutePath);
  }

  // Strip <!-- directive: value --> comments before rendering
  const cleanedMarkdown = markdown.replace(/<!--.*?-->/gs, "");

  return (
    <div className="instructions-panel">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          // Override img to resolve local asset paths
          img({ src, alt }) {
            return (
              <img
                src={resolveImageSrc(src)}
                alt={alt ?? ""}
                className="instructions-image"
              />
            );
          },
          // Style code blocks distinctly for SQL/command examples
          code({ children, className }) {
            const isBlock = className?.startsWith("language-");
            return isBlock ? (
              <pre className="instructions-code-block">
                <code>{children}</code>
              </pre>
            ) : (
              <code className="instructions-inline-code">{children}</code>
            );
          },
        }}
      >
        {cleanedMarkdown}
      </ReactMarkdown>
    </div>
  );
}