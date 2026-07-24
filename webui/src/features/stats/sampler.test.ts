// Copyright (C) 2026  namazso <admin@namazso.eu>
// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import { describe, expect, it } from 'vitest';
import { MetricSampler } from './sampler';

const counter = (name: string, value: number) => ({ name, kind: 'COUNTER' as const, value });
const gauge = (name: string, value: number) => ({ name, kind: 'GAUGE' as const, value });

describe('MetricSampler', () => {
  it('emits gauges raw and counters as rates with a leading gap', () => {
    const s = new MetricSampler();
    s.push(100, [counter('net.recv', 1000), gauge('dht.nodes', 50)]);
    s.push(101, [counter('net.recv', 3000), gauge('dht.nodes', 55)]);
    s.push(103, [counter('net.recv', 7000), gauge('dht.nodes', 60)]);
    const [times, recv, nodes] = s.aligned(['net.recv', 'dht.nodes']);
    expect(times).toEqual([100, 101, 103]);
    expect(recv).toEqual([null, 2000, 2000]); // Δ4000 over Δ2s
    expect(nodes).toEqual([50, 55, 60]);
  });

  it('gaps on counter reset instead of spiking', () => {
    const s = new MetricSampler();
    s.push(1, [counter('c', 5000)]);
    s.push(2, [counter('c', 6000)]);
    s.push(3, [counter('c', 100)]); // daemon restarted
    s.push(4, [counter('c', 200)]);
    const [, values] = s.aligned(['c']);
    expect(values).toEqual([null, 1000, null, 100]);
  });

  it('breakContinuity gaps the next counter delta', () => {
    const s = new MetricSampler();
    s.push(1, [counter('c', 100)]);
    s.push(2, [counter('c', 200)]);
    s.breakContinuity();
    s.push(10, [counter('c', 900)]);
    const [, values] = s.aligned(['c']);
    expect(values).toEqual([null, 100, null]);
  });

  it('trims to capacity and aligns late-appearing series', () => {
    const s = new MetricSampler(3);
    s.push(1, [gauge('a', 1)]);
    s.push(2, [gauge('a', 2), gauge('b', 20)]);
    s.push(3, [gauge('a', 3), gauge('b', 30)]);
    s.push(4, [gauge('a', 4), gauge('b', 40)]);
    const [times, a, b] = s.aligned(['a', 'b']);
    expect(times).toEqual([2, 3, 4]);
    expect(a).toEqual([2, 3, 4]);
    expect(b).toEqual([20, 30, 40]);
  });

  it('latest skips trailing gaps', () => {
    const s = new MetricSampler();
    s.push(1, [counter('c', 100)]);
    s.push(2, [counter('c', 300)]);
    expect(s.latest('c')).toBe(200);
    expect(s.latest('missing')).toBeNull();
  });
});
