// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * Ring-buffered metric sampling for the stats charts. COUNTER metrics are
 * converted to rates (Δvalue/Δt); the first sample after (re)subscribe and
 * counter resets (daemon restart) produce a null gap instead of a spike.
 * Plain class, zero React churn — charts read aligned arrays imperatively.
 */
export interface StatSample {
  name: string;
  kind: 'COUNTER' | 'GAUGE';
  value: number;
}

export class MetricSampler {
  private readonly capacity: number;
  /** Shared time axis (seconds, unix). */
  times: number[] = [];
  /** Per-series aligned values (null = gap). */
  private series = new Map<string, (number | null)[]>();
  private prevRaw = new Map<string, { value: number; time: number }>();

  constructor(capacity = 600) {
    this.capacity = capacity;
  }

  push(timeSec: number, samples: readonly StatSample[]): void {
    this.times.push(timeSec);
    const present = new Set<string>();
    for (const sample of samples) {
      present.add(sample.name);
      let values = this.series.get(sample.name);
      if (!values) {
        values = new Array<number | null>(this.times.length - 1).fill(null);
        this.series.set(sample.name, values);
      }
      if (sample.kind === 'GAUGE') {
        values.push(sample.value);
      } else {
        const prev = this.prevRaw.get(sample.name);
        if (prev && sample.value >= prev.value && timeSec > prev.time) {
          values.push((sample.value - prev.value) / (timeSec - prev.time));
        } else {
          values.push(null); // first sample or counter reset
        }
        this.prevRaw.set(sample.name, { value: sample.value, time: timeSec });
      }
    }
    // Series absent from this tick (name filter changed) get a gap.
    for (const [name, values] of this.series) {
      if (!present.has(name)) values.push(null);
    }
    if (this.times.length > this.capacity) {
      const drop = this.times.length - this.capacity;
      this.times.splice(0, drop);
      for (const values of this.series.values()) values.splice(0, drop);
    }
  }

  /** Mark a discontinuity (tab hidden, reconnect): next counter delta gaps. */
  breakContinuity(): void {
    this.prevRaw.clear();
  }

  /** uPlot AlignedData: [times, ...series in `names` order]. */
  aligned(names: readonly string[]): [number[], ...(number | null)[][]] {
    return [
      this.times,
      ...names.map(
        (n) => this.series.get(n) ?? new Array<number | null>(this.times.length).fill(null),
      ),
    ];
  }

  latest(name: string): number | null {
    const values = this.series.get(name);
    if (!values || values.length === 0) return null;
    for (let i = values.length - 1; i >= 0 && i >= values.length - 3; i--) {
      const v = values[i];
      if (v != null) return v;
    }
    return null;
  }
}
