export interface Destination {
  /** Where the link goes, always with a trailing slash. */
  readonly path: string;
  /** The link text, everywhere the destination is linked. */
  readonly label: string;
  /** One sentence, for the drawer and the home page's doors. */
  readonly blurb: string;
  /** The path prefix this destination owns, for marking the current one. */
  readonly covers?: string;
}

export declare const destinations: readonly Destination[];

export declare function destinationFor(path: string): Destination | undefined;
