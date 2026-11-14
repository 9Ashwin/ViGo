/** Keep localized names, original bundle names and filenames equally searchable. */
export function matchesApp(app: { name: string; path: string; aliases?: string[] }, query: string): boolean {
  const words = query.normalize('NFKC').trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const text = [app.name, app.path.split('/').pop() ?? '', ...app.aliases ?? []]
    .join(' ').normalize('NFKC').toLocaleLowerCase();
  return words.every((word) => text.includes(word));
}
