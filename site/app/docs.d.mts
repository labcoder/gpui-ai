export interface Doc {
  readonly slug: string;
  /** The page's heading, and the link text everywhere it is linked. */
  readonly title: string;
  /** One sentence, used for the page metadata and the index card. */
  readonly summary: string;
}

export declare const docs: readonly Doc[];

export declare function docBySlug(slug: string): Doc | undefined;
