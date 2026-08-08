import type { ComponentPropsWithoutRef } from "react"
import ReactMarkdown from "react-markdown"
import rehypeSanitize, {
  defaultSchema,
  type Options as SanitizeSchema,
} from "rehype-sanitize"
import remarkGfm from "remark-gfm"

import { cn } from "@/lib/utils"

import { sanitizeMarkdownUrl } from "./markdown-security"

const allowedElements = [
  "a",
  "blockquote",
  "br",
  "code",
  "del",
  "em",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "li",
  "ol",
  "p",
  "pre",
  "strong",
  "table",
  "tbody",
  "td",
  "th",
  "thead",
  "tr",
  "ul",
] as const

const markdownSanitizeSchema: SanitizeSchema = {
  ...defaultSchema,
  tagNames: [...allowedElements],
  attributes: {
    a: ["href", "title"],
    code: [["className", /^language-[\w-]+$/]],
    ol: ["start"],
    td: ["align"],
    th: ["align"],
  },
  protocols: {
    href: ["http", "https", "mailto"],
  },
  clobberPrefix: "kokorokoe-markdown-",
}

const markdownClasses = [
  "space-y-3 text-sm leading-6 text-foreground",
  "[&_h1]:text-2xl [&_h1]:font-semibold [&_h2]:text-xl [&_h2]:font-semibold",
  "[&_h3]:text-lg [&_h3]:font-semibold [&_h4]:font-semibold [&_h5]:font-semibold [&_h6]:font-semibold",
  "[&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground",
  "[&_ul]:list-disc [&_ul]:pl-6 [&_ol]:list-decimal [&_ol]:pl-6",
  "[&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:bg-muted [&_pre]:p-3",
  "[&_:not(pre)>code]:rounded [&_:not(pre)>code]:bg-muted [&_:not(pre)>code]:px-1 [&_:not(pre)>code]:py-0.5",
  "[&_table]:w-full [&_table]:border-collapse [&_th]:border [&_th]:p-2 [&_th]:text-left [&_td]:border [&_td]:p-2",
].join(" ")

function InertExternalLink({ children, href }: ComponentPropsWithoutRef<"a">) {
  return (
    <span
      className="text-primary underline decoration-dotted underline-offset-4"
      data-markdown-link={href ? "allowed" : "blocked"}
      title={
        href
          ? "External links are disabled in this foundation build."
          : "This link uses a blocked address."
      }
    >
      {children}
    </span>
  )
}

type SanitizedMarkdownProps = {
  children: string
  className?: string
}

export function SanitizedMarkdown({
  children,
  className,
}: SanitizedMarkdownProps) {
  return (
    <div className={cn(markdownClasses, className)} data-sanitized-markdown>
      <ReactMarkdown
        allowedElements={allowedElements}
        components={{ a: InertExternalLink }}
        rehypePlugins={[[rehypeSanitize, markdownSanitizeSchema]]}
        remarkPlugins={[remarkGfm]}
        skipHtml
        urlTransform={sanitizeMarkdownUrl}
      >
        {children}
      </ReactMarkdown>
    </div>
  )
}
