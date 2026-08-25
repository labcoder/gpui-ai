/** A demo that can be told whether it may run. */
export interface Seat {
  live(running: boolean): void;
}

export declare const LIVE_LIMIT: number;

export declare function wantSeat(seat: Seat, away: number): void;

export declare function dropSeat(seat: Seat): void;

export declare function seated(): readonly Seat[];

export declare function clearSeats(): void;

export declare function distanceAway(
  box: { readonly top: number; readonly bottom: number },
  viewportHeight: number,
): number;
