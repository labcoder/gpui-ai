/**
 * What the ranking needs from a record, and nothing else.
 *
 * Generic over the record so the index can hold components and decorations
 * without the ranking knowing which is which — it scores fields, and a field
 * a decoration does not have is empty rather than absent.
 */
export interface Searchable {
  readonly api?: string;
  readonly title?: string;
  readonly compactLabel?: string;
  readonly events?: readonly string[];
  readonly category?: string;
  readonly summary?: string;
  readonly usage?: string;
  readonly behavior?: Readonly<Record<string, string>>;
}

export interface IndexEntry<T extends Searchable = Searchable> {
  readonly component: T;
  readonly fields: Readonly<Record<string, string>>;
}

export declare function buildIndex<T extends Searchable>(
  records: readonly T[],
): readonly IndexEntry<T>[];

export declare function search<T extends Searchable>(
  index: readonly IndexEntry<T>[],
  query: string,
): readonly T[];
