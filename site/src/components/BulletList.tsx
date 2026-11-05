/** Points that belong together but in no particular order. */
export function BulletList({ items, className = "" }: { items: string[]; className?: string }) {
  return (
    <ul className={`list-disc space-y-3 pl-5 marker:text-ink-muted ${className}`}>
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}
