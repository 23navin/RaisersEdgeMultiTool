// StepImport.tsx
//
// Renders steps with type=manual_instruction. Flat prose, no card wrapper:
// paragraphs and image blocks rendered in markdown document order.

import { useEffect, useState } from "react";
import { ImageIcon } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";

type Props = {
  markdown: string;
  tempDir: string;
};

type Block =
  | { kind: "paragraph"; text: string }
  | { kind: "image"; alt: string; src: string };

// Walks the markdown line-by-line, grouping non-image lines into paragraphs
// and emitting image blocks (`![alt](src)`) as their own entries.
function parseBlocks(md: string): Block[] {
  const blocks: Block[] = [];
  let buffer: string[] = [];

  const flush = () => {
    const text = buffer.join("\n").trim();
    if (text) blocks.push({ kind: "paragraph", text });
    buffer = [];
  };

  for (const rawLine of md.split("\n")) {
    const line = rawLine.trim();
    const imageMatch = line.match(/^!\[([^\]]*)\]\(([^)]+)\)\s*$/);
    if (imageMatch) {
      flush();
      blocks.push({ kind: "image", alt: imageMatch[1], src: imageMatch[2] });
      continue;
    }
    if (line === "") {
      flush();
      continue;
    }
    buffer.push(line);
  }
  flush();
  return blocks;
}

function renderInlineMd(text: string) {
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          className="font-mono text-[12px] bg-neutral-100 px-[5px] py-[1px] rounded border border-neutral-200"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

function ImageBlock({ alt, src, tempDir }: { alt: string; src: string; tempDir: string }) {
  const [resolvedSrc, setResolvedSrc] = useState<string | null>(null);
  const [errored, setErrored] = useState(false);

  useEffect(() => {
    if (!tempDir) {
      setResolvedSrc(null);
      return;
    }
    try {
      // Convert the OS path to an asset URL Tauri can serve.
      const full = `${tempDir}/${src}`.replace(/\\/g, "/");
      setResolvedSrc(convertFileSrc(full));
    } catch {
      setErrored(true);
    }
  }, [src, tempDir]);

  if (!resolvedSrc || errored) {
    return (
      <div className="h-[80px] rounded-md border border-neutral-200 bg-neutral-50 flex items-center justify-center gap-[8px] text-neutral-400 text-[12px] my-[10px]">
        <ImageIcon size={18} />
        <span>{alt}</span>
      </div>
    );
  }

  return (
    <img
      src={resolvedSrc}
      alt={alt}
      onError={() => setErrored(true)}
      className="rounded-md w-full my-[10px] border border-neutral-200"
    />
  );
}

export function StepImport({ markdown, tempDir }: Props) {
  const blocks = parseBlocks(markdown);

  return (
    <div>
      {blocks.map((block, i) => {
        if (block.kind === "image") {
          return (
            <ImageBlock
              key={i}
              alt={block.alt}
              src={block.src}
              tempDir={tempDir}
            />
          );
        }
        return (
          <p
            key={i}
            className="text-[13px] text-neutral-500 leading-relaxed"
          >
            {renderInlineMd(block.text)}
          </p>
        );
      })}
    </div>
  );
}
