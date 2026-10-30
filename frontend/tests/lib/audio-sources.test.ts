import { describe, expect, test } from 'bun:test';
import { autoOutput, effectiveDevice, levelPercent, PeakHistory, screenChoiceKey, type EndpointLevel } from '../../src/lib/audio-sources';

const out = (name: string, peak: number, isDefault = false, isCommunications = false): EndpointLevel => ({ name, peak, isDefault, isCommunications });

describe('recording sources', () => {
  test('Automatique follows the backend rule', () => {
    const speakers = out('Haut-parleurs (Realtek(R) Audio)', 0, true);
    const headset = out('Casque (2- Jabra Evolve 65 SE)', 0, false, true);
    const peaks = (values: Record<string, number>) => (name: string) => values[name] ?? 0;
    // 23/10 setup, nothing playing yet: the call output
    expect(autoOutput([speakers, headset], peaks({}))).toBe(headset.name);
    // Teams playing in the headset
    expect(autoOutput([speakers, headset], peaks({ [headset.name]: 0.2 }))).toBe(headset.name);
    // Browser meeting on the speakers only
    expect(autoOutput([speakers, headset], peaks({ [speakers.name]: 0.3 }))).toBe(speakers.name);
    // One device for both roles
    expect(autoOutput([out('Casque', 0, true, true)], peaks({}))).toBe('Casque');
    expect(autoOutput([], peaks({}))).toBeNull();
  });

  test('a remembered device that is unplugged is never sent', () => {
    const plugged = ['Micro Jabra (input)', 'Réseau de microphones (input)'];
    expect(effectiveDevice('Micro Jabra (input)', plugged)).toEqual({ value: 'Micro Jabra (input)', missing: null });
    expect(effectiveDevice('Micro USB (input)', plugged)).toEqual({ value: null, missing: 'Micro USB (input)' });
    expect(effectiveDevice(null, plugged)).toEqual({ value: null, missing: null });
    // Device list not loaded yet: nothing is decided
    expect(effectiveDevice('Micro USB (input)', null)).toEqual({ value: 'Micro USB (input)', missing: null });
  });

  test('screens are remembered by position', () => {
    const monitors = [
      { id: 'monitor:333', key: 'pos:-1920,0' },
      { id: 'monitor:111', key: 'pos:0,0' },
    ];
    expect(screenChoiceKey('pos:-1920,0', monitors)).toEqual({ key: 'pos:-1920,0', missing: false });
    expect(screenChoiceKey('monitor:111', monitors)).toEqual({ key: 'pos:0,0', missing: false });
    expect(screenChoiceKey('pos:3840,0', monitors)).toEqual({ key: 'primary', missing: true });
    expect(screenChoiceKey('all', monitors)).toEqual({ key: 'all', missing: false });
    expect(screenChoiceKey('pos:3840,0', null)).toEqual({ key: 'pos:3840,0', missing: false });
  });

  test('gauges use a decibel scale and recent peaks', () => {
    expect(levelPercent(0)).toBe(0);
    expect(levelPercent(1)).toBe(100);
    expect(Math.round(levelPercent(0.001))).toBe(0);
    expect(Math.round(levelPercent(0.1))).toBe(67);
    const history = new PeakHistory(1500);
    history.add([out('A', 0.3)], 0);
    history.add([out('A', 0.01)], 1000);
    expect(history.recent('A')).toBe(0.3);
    history.add([out('A', 0.02)], 2000);
    expect(history.recent('A')).toBe(0.02);
    expect(history.recent('B')).toBe(0);
  });
});
