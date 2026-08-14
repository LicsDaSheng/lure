import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

/** assistant 内容与 reasoning 共用的 markdown 组件映射。 */
export const markdownComponents: Components = {
  a: (props) => (
    <a
      {...props}
      target="_blank"
      rel="noreferrer"
      className="text-accent underline underline-offset-2"
    />
  ),
  code: ({ className, children, ...props }) => {
    const inline = !className;
    return inline ? (
      <code
        className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.85em]"
        {...props}
      >
        {children}
      </code>
    ) : (
      <code className={className} {...props}>
        {children}
      </code>
    );
  },
  pre: (props) => (
    <pre
      {...props}
      className="my-3 overflow-x-auto rounded-lg border bg-muted/60 p-3 font-mono text-[0.82rem] leading-relaxed"
    />
  ),
};

/** assistant 内容 markdown 渲染。样式走 tailwind 手写规则，避免额外插件依赖。 */
export function Markdown({ children }: { children: string }) {
  return (
    <div className="prose-chat">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
        {children}
      </ReactMarkdown>
    </div>
  );
}
