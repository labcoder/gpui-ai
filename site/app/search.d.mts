import type { Component } from "./data";

export interface IndexEntry {
  readonly component: Component;
  readonly fields: Readonly<Record<string, string>>;
}

export declare function buildIndex(components: readonly Component[]): readonly IndexEntry[];

export declare function search(
  index: readonly IndexEntry[],
  query: string,
): readonly Component[];
