export declare const SYSTEM: "system";

export declare const DEFAULT: string;

export declare function resolveChoice(input?: {
  param?: string | null | undefined;
  stored?: string | null | undefined;
  fallback?: string | null | undefined;
}): string;

export declare function appliedTheme(choice: string, prefersDark: boolean): string;

export declare function isDarkTheme(applied: string, darkSlugs: ReadonlySet<string>): boolean;
