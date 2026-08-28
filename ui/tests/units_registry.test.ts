import { describe, expect, it } from 'vitest';
import { allSpellings, allUnits, findUnit, unitsOf } from '../src/units/registry.ts';
import { convertValue } from '../src/units/convert.ts';
import { Dimension, Unit } from '../src/units/types.ts';
import { FEATURES_DOC } from './support/sources.ts';

/** The row every other row in its dimension is measured against. */
const BASE_UNIT: Record<Dimension, string> = {
  length: 'm',
  mass: 'g',
  volume: 'mL',
  temperature: 'K',
  time: 's',
  area: 'm²',
  digital: 'B',
  speed: 'm/s',
};

const DIMENSIONS: Dimension[] = [
  'length',
  'mass',
  'volume',
  'temperature',
  'time',
  'area',
  'digital',
  'speed',
];

/** Converts, or fails the test saying why it could not. */
function convert(value: number, from: string, to: string): number {
  const source = findUnit(from);
  const target = findUnit(to);
  expect(source, from).not.toBeNull();
  expect(target, to).not.toBeNull();
  const result = convertValue(value, source!, target!);
  expect(result.ok ? '' : result.failure).toBe('');
  return result.ok ? result.value : Number.NaN;
}

describe('the table is well formed', () => {
  it('gives every unit exactly one dimension and one way of converting', () => {
    for (const unit of allUnits()) {
      expect(DIMENSIONS, unit.id).toContain(unit.dimension);

      const linear = unit.scale !== undefined;
      const custom = unit.toBase !== undefined && unit.fromBase !== undefined;
      // One or the other, never both and never neither: a row with a scale and
      // converters would have two answers to the same question.
      expect(linear !== custom, `${unit.id} must be linear or converted, not both`).toBe(true);
      if (linear) expect(unit.scale, unit.id).toBeGreaterThan(0);
    }
  });

  it('gives every dimension a base unit, and the documented one', () => {
    for (const dimension of DIMENSIONS) {
      const units = unitsOf(dimension);
      expect(units.length, dimension).toBeGreaterThan(1);

      const base = findUnit(BASE_UNIT[dimension])!;
      expect(base.dimension, dimension).toBe(dimension);
      // The base is the row the others are measured against, so it converts
      // to itself by doing nothing at all.
      expect(base.scale !== undefined ? base.scale : base.toBase!(1), dimension).toBe(1);
    }
  });

  it('has more than one row at scale 1 only where two units are the same size', () => {
    // `cm³` and `mL` are equal by the definition of the litre, so both sit at
    // the volume base. Anywhere else that would be two names for one row.
    for (const dimension of DIMENSIONS) {
      const ones = unitsOf(dimension).filter((unit) => unit.scale === 1 || unit.toBase?.(1) === 1);
      const expected = dimension === 'volume' ? ['mL', 'cm³'] : [BASE_UNIT[dimension]];
      expect(ones.map((unit) => unit.id).sort(), dimension).toEqual([...expected].sort());
    }
  });

  it('claims no spelling twice', () => {
    // The registry throws while building if two rows claim one spelling, so
    // reaching this point already proves it. Counting keeps the guarantee
    // visible rather than buried in a module initialiser.
    const spellings = allSpellings();
    expect(new Set(spellings).size).toBe(spellings.length);
  });

  it('resolves a spelling exactly, with no case folding of its own', () => {
    expect(findUnit('m')?.id).toBe('m');
    expect(findUnit('km')?.id).toBe('km');
    // `M` is not a unit at all. Folding it onto `m` would make `MB` and `mb`
    // one thing too, and those differ by a factor of eight million.
    expect(findUnit('M')).toBeNull();
    expect(findUnit('KM')).toBeNull();
    expect(findUnit('mb')).toBeNull();
    expect(findUnit('Mb')).toBeNull();
    // ...while a lower-case litre is accepted because the table says so.
    expect(findUnit('ml')?.id).toBe('mL');
    expect(findUnit('l')?.id).toBe('L');
    expect(findUnit('MB')?.id).toBe('MB');
  });

  it('is reached through a Map, so no note can name a JavaScript property', () => {
    for (const name of ['constructor', '__proto__', 'toString', 'valueOf', 'hasOwnProperty']) {
      expect(findUnit(name), name).toBeNull();
    }
  });

  it('has a display symbol for every unit and a plural only where one reads', () => {
    for (const unit of allUnits()) {
      expect(unit.symbol.length, unit.id).toBeGreaterThan(0);
    }
    const plurals = allUnits().filter((unit) => unit.plural !== undefined);
    expect(plurals.map((unit) => unit.id)).toEqual(['dia', 'semana']);
  });
});

describe('the factors are the defined ones', () => {
  /** Every exact definition the table leans on, checked against its source. */
  const EXACT: Array<[string, number]> = [
    ['in', 0.0254], // international inch, 1959
    ['ft', 0.3048],
    ['yd', 0.9144],
    ['mi', 1609.344],
    ['lb', 453.59237], // avoirdupois pound
    ['oz', 453.59237 / 16],
    ['mph', 1609.344 / 3600],
  ];

  it.each(EXACT)('defines %s exactly', (id, expected) => {
    expect(findUnit(id)!.scale).toBe(expected);
  });

  it('squares the linear factor for every area unit', () => {
    // An area unit is not its length unit; `1 m²` is `10 000 cm²`, not `100`.
    const pairs: Array<[string, string]> = [
      ['mm', 'mm²'],
      ['cm', 'cm²'],
      ['m', 'm²'],
      ['km', 'km²'],
    ];
    for (const [length, area] of pairs) {
      expect(findUnit(area)!.scale, area).toBeCloseTo(findUnit(length)!.scale! ** 2, 15);
    }
  });

  it('keeps the SI and the IEC prefixes apart', () => {
    expect(findUnit('KB')!.scale).toBe(1000);
    expect(findUnit('MB')!.scale).toBe(1000 ** 2);
    expect(findUnit('GB')!.scale).toBe(1000 ** 3);
    expect(findUnit('TB')!.scale).toBe(1000 ** 4);
    expect(findUnit('KiB')!.scale).toBe(1024);
    expect(findUnit('MiB')!.scale).toBe(1024 ** 2);
    expect(findUnit('GiB')!.scale).toBe(1024 ** 3);
    expect(findUnit('TiB')!.scale).toBe(1024 ** 4);
  });

  it('makes a cubic centimetre a millilitre, by definition', () => {
    expect(convert(1, 'cm³', 'mL')).toBe(1);
    expect(convert(1, 'm³', 'L')).toBe(1000);
  });
});

describe('conversion itself', () => {
  it('carries a value through the base and back', () => {
    expect(convert(1, 'km', 'm')).toBe(1000);
    expect(convert(1000, 'm', 'km')).toBe(1);
    expect(convert(1, 'kg', 'g')).toBe(1000);
    expect(convert(1, 'L', 'mL')).toBe(1000);
  });

  it('returns the same number for a unit converted to itself', () => {
    // Every conversion takes the same route through the base, including this
    // one, so a scale that is not a power of two leaves a few bits behind:
    // `7 cm` comes back as 7.000000000000001. That is far below the twelve
    // significant digits a result is shown to, and going round the check for
    // absolute zero to avoid it would be trading a guarantee for a rounding.
    for (const unit of allUnits()) {
      const result = convertValue(7, unit, unit);
      expect(result.ok, unit.id).toBe(true);
      expect(result.ok ? result.value : NaN, unit.id).toBeCloseTo(7, 12);
    }
  });

  it('round-trips every unit through its base without drifting', () => {
    for (const unit of allUnits()) {
      const there = convert(3, unit.id, BASE_UNIT[unit.dimension]);
      const back = convert(there, BASE_UNIT[unit.dimension], unit.id);
      expect(back, unit.id).toBeCloseTo(3, 9);
    }
  });

  it('refuses two units that were never comparable', () => {
    const kg = findUnit('kg')!;
    const km = findUnit('km')!;
    expect(convertValue(10, kg, km)).toEqual({ ok: false, failure: 'incompatible' });
    expect(convertValue(10, km, kg)).toEqual({ ok: false, failure: 'incompatible' });
  });

  it('treats temperature as scales with different zeroes, not as a factor', () => {
    // The whole reason temperature carries converters: no multiplication takes
    // 0 to 32 and 100 to 212 at the same time.
    expect(convert(0, '°C', '°F')).toBeCloseTo(32, 10);
    expect(convert(100, '°C', '°F')).toBeCloseTo(212, 10);
    expect(convert(32, '°F', '°C')).toBeCloseTo(0, 10);
    expect(convert(0, '°C', 'K')).toBeCloseTo(273.15, 10);
    expect(convert(-40, '°C', '°F')).toBeCloseTo(-40, 10);
  });

  it('will not convert a temperature that is under absolute zero', () => {
    const celsius = findUnit('°C')!;
    const kelvin = findUnit('K')!;
    expect(convertValue(-300, celsius, kelvin)).toEqual({ ok: false, failure: 'impossible' });
    expect(convertValue(-1, kelvin, celsius)).toEqual({ ok: false, failure: 'impossible' });
    // Absolute zero itself is a real reading.
    expect(convertValue(-273.15, celsius, kelvin).ok).toBe(true);
  });
});

describe('what the table deliberately does not hold', () => {
  it('has no unit whose value depends on who is asking', () => {
    // A cup is 240 mL, 236.5882365 mL or 250 mL depending on the cookbook, and
    // an alqueire depends on the state. A conversion that is silently wrong is
    // worse than one that is absent.
    for (const name of ['cup', 'xicara', 'tsp', 'tbsp', 'colher', 'alqueire']) {
      expect(findUnit(name), name).toBeNull();
    }
  });

  it('has no currency, and no dimension that would need a network to answer', () => {
    for (const name of ['USD', 'BRL', 'EUR', 'usd', 'brl', 'eur', 'BTC']) {
      expect(findUnit(name), name).toBeNull();
    }
    const dimensions = new Set(allUnits().map((unit: Unit) => unit.dimension));
    expect(dimensions.has('currency' as Dimension)).toBe(false);
  });
});

describe('the documented table is the shipped table', () => {
  it('documents every unit and every alias, and invents none', () => {
    // `docs/features.md` writes the units out in full rather than saying that
    // conversions exist. A table nobody can trust is worse than no table, so
    // the document and the registry are checked against each other.
    const documented = new Set(
      [...FEATURES_DOC.matchAll(/`([^`\n]+)`/g)].map((match) => match[1]),
    );

    for (const spelling of allSpellings()) {
      expect(documented.has(spelling), `${spelling} is not in docs/features.md`).toBe(true);
    }
  });

  it('states the base unit of every dimension', () => {
    for (const dimension of DIMENSIONS) {
      expect(FEATURES_DOC, dimension).toContain(`base \`${BASE_UNIT[dimension]}\``);
    }
  });

  it('documents the factor each unit actually converts by', () => {
    // The documented factor is read back out of the table and compared as a
    // number, so it may be written the way a person reads it — `1/3,6` — and
    // still be checked against the one the engine uses.
    const rows = new Map<string, string>();
    for (const row of FEATURES_DOC.matchAll(/^\| `(.+?)` \| .*? \| .*? \| (.+?) \|$/gm)) {
      rows.set(row[1], row[2].trim());
    }

    for (const unit of allUnits()) {
      if (unit.scale === undefined) continue;
      const written = rows.get(unit.id);
      expect(written, `${unit.id} has no row in docs/features.md`).toBeDefined();

      const normalized = written!.replace(/,/g, '.');
      const fraction = /^([\d.]+)\/([\d.]+)$/.exec(normalized);
      const documented = fraction
        ? Number.parseFloat(fraction[1]) / Number.parseFloat(fraction[2])
        : Number.parseFloat(normalized);

      expect(documented / unit.scale, `${unit.id}: doc says ${written}`).toBeCloseTo(1, 10);
    }
  });
});
