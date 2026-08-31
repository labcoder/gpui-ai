export interface Redirect {
  /** The path that used to be a page, always with a trailing slash. */
  readonly from: string;
  /** Where it went, base-relative and with a trailing slash. */
  readonly to: string;
}

export declare const redirects: readonly Redirect[];
