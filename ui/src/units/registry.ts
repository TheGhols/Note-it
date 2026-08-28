import { Dimension, Unit } from './types.ts';

/**
 * Every unit Note-it knows, and every spelling it answers to.
 *
 * A table rather than a chain of conditionals, for the reason every conversion
 * library eventually learns: `if km then m, if m then cm, …` is O(n²) rules to
 * write and O(n²) rules to get wrong, while a scale per unit is one number per
 * row and the conversion is arithmetic.
 *
 * Each dimension has a base unit, which is simply the row whose scale is 1:
 *
 * | dimension | base |
 * | --- | --- |
 * | length | `m` |
 * | mass | `g` |
 * | volume | `mL` |
 * | temperature | `K` |
 * | time | `s` |
 * | area | `m²` |
 * | digital | `B` |
 * | speed | `m/s` |
 *
 * Mass is based on the gram rather than the SI kilogram so that `mg` is
 * `0.001` and not `0.000001`; the base is an implementation detail of this
 * table and nothing outside it depends on the choice.
 *
 * **Every factor here is exact**, and the ones that look arbitrary are
 * definitions rather than measurements: an inch is 0.0254 m by international
 * agreement, a pound is 453.59237 g, a mile is 1609.344 m. Nothing was rounded
 * to make a table entry tidier, and nothing was included whose value depends on
 * who is asking — which is why there is no `cup`, no `tsp` and no `alqueire`
 * here. See the note at the bottom of this file.
 */
const UNITS: readonly Unit[] = [
  // ---------------------------------------------------------------- length
  { id: 'mm', symbol: 'mm', dimension: 'length', scale: 0.001, aliases: ['milimetro', 'milimetros'] },
  { id: 'cm', symbol: 'cm', dimension: 'length', scale: 0.01, aliases: ['centimetro', 'centimetros'] },
  { id: 'm', symbol: 'm', dimension: 'length', scale: 1, aliases: ['metro', 'metros'] },
  { id: 'km', symbol: 'km', dimension: 'length', scale: 1000, aliases: ['quilometro', 'quilometros'] },
  // Imperial lengths, all exact by the 1959 international agreement.
  { id: 'in', symbol: 'in', dimension: 'length', scale: 0.0254, aliases: ['polegada', 'polegadas'] },
  { id: 'ft', symbol: 'ft', dimension: 'length', scale: 0.3048, aliases: ['pe', 'pes'] },
  { id: 'yd', symbol: 'yd', dimension: 'length', scale: 0.9144, aliases: ['jarda', 'jardas'] },
  { id: 'mi', symbol: 'mi', dimension: 'length', scale: 1609.344, aliases: ['milha', 'milhas'] },

  // ------------------------------------------------------------------ mass
  { id: 'mg', symbol: 'mg', dimension: 'mass', scale: 0.001, aliases: ['miligrama', 'miligramas'] },
  { id: 'g', symbol: 'g', dimension: 'mass', scale: 1, aliases: ['grama', 'gramas'] },
  { id: 'kg', symbol: 'kg', dimension: 'mass', scale: 1000, aliases: ['quilograma', 'quilogramas', 'quilo', 'quilos'] },
  { id: 't', symbol: 't', dimension: 'mass', scale: 1_000_000, aliases: ['tonelada', 'toneladas'] },
  // Avoirdupois, exact: 1 lb = 453.59237 g, 1 oz = 1 lb / 16.
  { id: 'oz', symbol: 'oz', dimension: 'mass', scale: 28.349523125, aliases: ['onca', 'oncas'] },
  { id: 'lb', symbol: 'lb', dimension: 'mass', scale: 453.59237, aliases: ['libra', 'libras'] },

  // ---------------------------------------------------------------- volume
  { id: 'mL', symbol: 'mL', dimension: 'volume', scale: 1, aliases: ['ml', 'mililitro', 'mililitros'] },
  { id: 'cL', symbol: 'cL', dimension: 'volume', scale: 10, aliases: ['cl', 'centilitro', 'centilitros'] },
  { id: 'dL', symbol: 'dL', dimension: 'volume', scale: 100, aliases: ['dl', 'decilitro', 'decilitros'] },
  { id: 'L', symbol: 'L', dimension: 'volume', scale: 1000, aliases: ['l', 'litro', 'litros'] },
  // A cubic centimetre is a millilitre exactly, by definition of the litre.
  { id: 'cm³', symbol: 'cm³', dimension: 'volume', scale: 1, aliases: ['cm3', 'cc'] },
  { id: 'm³', symbol: 'm³', dimension: 'volume', scale: 1_000_000, aliases: ['m3'] },

  // ----------------------------------------------------------- temperature
  // Not linear: the three scales have three different zeroes, so these rows
  // carry converters instead of a factor. The base is the kelvin.
  {
    id: '°C',
    symbol: '°C',
    dimension: 'temperature',
    aliases: ['C', 'c', 'celsius'],
    toBase: (value) => value + 273.15,
    fromBase: (value) => value - 273.15,
  },
  {
    id: '°F',
    symbol: '°F',
    dimension: 'temperature',
    aliases: ['F', 'f', 'fahrenheit'],
    toBase: (value) => ((value + 459.67) * 5) / 9,
    fromBase: (value) => (value * 9) / 5 - 459.67,
  },
  {
    id: 'K',
    symbol: 'K',
    dimension: 'temperature',
    aliases: ['kelvin'],
    toBase: (value) => value,
    fromBase: (value) => value,
  },

  // ------------------------------------------------------------------ time
  { id: 'ms', symbol: 'ms', dimension: 'time', scale: 0.001, aliases: ['milissegundo', 'milissegundos'] },
  { id: 's', symbol: 's', dimension: 'time', scale: 1, aliases: ['seg', 'segundo', 'segundos'] },
  { id: 'min', symbol: 'min', dimension: 'time', scale: 60, aliases: ['minuto', 'minutos'] },
  { id: 'h', symbol: 'h', dimension: 'time', scale: 3600, aliases: ['hora', 'horas'] },
  { id: 'dia', symbol: 'dia', plural: 'dias', dimension: 'time', scale: 86_400, aliases: ['dias', 'd'] },
  {
    id: 'semana',
    symbol: 'semana',
    plural: 'semanas',
    dimension: 'time',
    scale: 604_800,
    aliases: ['semanas'],
  },

  // ------------------------------------------------------------------ area
  // An area factor is the square of the linear one, written out rather than
  // derived, because `m²` is its own unit here and not `m` with an exponent.
  { id: 'mm²', symbol: 'mm²', dimension: 'area', scale: 0.000001, aliases: ['mm2'] },
  { id: 'cm²', symbol: 'cm²', dimension: 'area', scale: 0.0001, aliases: ['cm2'] },
  { id: 'm²', symbol: 'm²', dimension: 'area', scale: 1, aliases: ['m2'] },
  { id: 'km²', symbol: 'km²', dimension: 'area', scale: 1_000_000, aliases: ['km2'] },
  { id: 'ha', symbol: 'ha', dimension: 'area', scale: 10_000, aliases: ['hectare', 'hectares'] },

  // --------------------------------------------------------------- digital
  // SI prefixes are decimal and IEC prefixes are binary, which is what the
  // two sets of names were created to distinguish. Note-it does not blur them:
  // `KB` is 1000 bytes and `KiB` is 1024, always.
  { id: 'B', symbol: 'B', dimension: 'digital', scale: 1, aliases: ['byte', 'bytes'] },
  { id: 'KB', symbol: 'KB', dimension: 'digital', scale: 1000, aliases: [] },
  { id: 'MB', symbol: 'MB', dimension: 'digital', scale: 1_000_000, aliases: [] },
  { id: 'GB', symbol: 'GB', dimension: 'digital', scale: 1_000_000_000, aliases: [] },
  { id: 'TB', symbol: 'TB', dimension: 'digital', scale: 1_000_000_000_000, aliases: [] },
  { id: 'KiB', symbol: 'KiB', dimension: 'digital', scale: 1024, aliases: [] },
  { id: 'MiB', symbol: 'MiB', dimension: 'digital', scale: 1_048_576, aliases: [] },
  { id: 'GiB', symbol: 'GiB', dimension: 'digital', scale: 1_073_741_824, aliases: [] },
  { id: 'TiB', symbol: 'TiB', dimension: 'digital', scale: 1_099_511_627_776, aliases: [] },

  // ----------------------------------------------------------------- speed
  // Three named rows, not `length / time` worked out at run time. A derived
  // dimension system is a physics library; this is a table with three lines.
  { id: 'm/s', symbol: 'm/s', dimension: 'speed', scale: 1, aliases: [] },
  { id: 'km/h', symbol: 'km/h', dimension: 'speed', scale: 1 / 3.6, aliases: [] },
  { id: 'mph', symbol: 'mph', dimension: 'speed', scale: 0.44704, aliases: [] },
];

/**
 * Every accepted spelling, mapped to its unit.
 *
 * A `Map`, and for the same reason the math engine's variables are one: an
 * object would answer `constructor`, `__proto__` and `toString` with real
 * JavaScript values, so `= 10 constructor em m` would be reaching into the
 * runtime instead of failing. Nothing here is ever indexed dynamically.
 *
 * Lookup is **exact and case-sensitive**. `m` is a metre and `M` is nothing;
 * `mL` and `ml` are both the millilitre because both are listed, and `mb` is
 * not the megabyte because it is not. There is no case folding and no
 * normalisation anywhere: every spelling Note-it accepts is a spelling written
 * down in the table above, which is what makes "is this a unit?" a question
 * with one answer.
 */
const BY_SPELLING: ReadonlyMap<string, Unit> = (() => {
  const index = new Map<string, Unit>();
  for (const unit of UNITS) {
    for (const spelling of [unit.id, ...unit.aliases]) {
      const existing = index.get(spelling);
      if (existing) {
        throw new Error(`note-it: "${spelling}" is claimed by both ${existing.id} and ${unit.id}`);
      }
      index.set(spelling, unit);
    }
  }
  return index;
})();

/** The unit written as `text`, or `null` when nothing in the table is spelled that way. */
export function findUnit(text: string): Unit | null {
  return BY_SPELLING.get(text) ?? null;
}

/** Every unit, in table order. Used by the documentation tests. */
export function allUnits(): readonly Unit[] {
  return UNITS;
}

/** Every spelling that resolves to a unit. Used by the documentation tests. */
export function allSpellings(): readonly string[] {
  return [...BY_SPELLING.keys()];
}

export function unitsOf(dimension: Dimension): readonly Unit[] {
  return UNITS.filter((unit) => unit.dimension === dimension);
}

/*
 * What is deliberately not here, and why.
 *
 * `cup`, `tsp`, `tbsp`, `xícara` and `colher` are all real measurements with
 * more than one real value — a US legal cup is 240 mL, a US customary cup is
 * 236.5882365 mL, a metric one is 250 mL, and a Brazilian recipe means
 * whichever the author had in the cupboard. `alqueire` differs by state.
 * A conversion whose answer depends on which definition the reader had in mind
 * is worse than no conversion, because it is wrong silently.
 *
 * Bits are absent for a smaller reason: `b` and `B` differing by a factor of
 * eight is a trap in a note nobody proofreads at that resolution.
 *
 * Currencies are absent for a different reason entirely. See `convert.ts`.
 */
