import { describe, expect, it } from 'vitest';
import { evaluateNote, MathSource } from '../src/math/document.ts';

/** Every result the note shows, one per line. */
function shown(lines: readonly MathSource[]): (string | null)[] {
  return evaluateNote(lines).map((result) => {
    if (result.kind === 'value') return result.text;
    if (result.kind === 'error') return `!${result.code}`;
    return null;
  });
}

/** The result of a single calculation line, as the reader would read it. */
function line(source: string): string | null {
  return shown([source])[0];
}

describe('the conversion grammar', () => {
  it('reads "expression unit em unit"', () => {
    expect(line('= 10 km em m')).toBe('10000 m');
    expect(line('= 500 cm em m')).toBe('5 m');
    expect(line('= 1500 m em km')).toBe('1,5 km');
  });

  it('accepts an arithmetic expression on the left', () => {
    expect(line('= (10 + 5) km em m')).toBe('15000 m');
    expect(line('= (5 + 5) km em m')).toBe('10000 m');
    expect(line('= 2 * 3 km em m')).toBe('6000 m');
    expect(line('= 100 / 4 cm em mm')).toBe('250 mm');
  });

  it('applies the unit to the whole expression, not to the term beside it', () => {
    // There is no unit algebra here, so one rule is stated and tested rather
    // than two the reader would have to guess between.
    expect(line('= 10 + 5 km em m')).toBe('15000 m');
  });

  it('accepts a variable, and an expression over variables', () => {
    expect(shown(['distancia := 12', '= distancia km em m'])).toEqual([null, '12000 m']);
    // The declaration computes, so it shows its own value too — the rule the
    // math engine has had since it existed, unchanged by conversions.
    expect(shown(['distancia := 10 + 5', '= distancia km em m'])).toEqual(['15', '15000 m']);
    expect(shown(['x := 5', '= x * 2 km em m'])).toEqual([null, '10000 m']);
  });

  it('lets a declaration hold a conversion, storing the number it came to', () => {
    // The variable holds 10000, not "10000 metres": a unit in a variable is
    // not something this phase implements, and pretending otherwise would be
    // the inconsistent half-measure.
    expect(shown(['metros := 10 km em m', '= metros * 2'])).toEqual(['10000 m', '20000']);
  });

  it('reads a compound unit written with a slash', () => {
    expect(line('= 100 km/h em m/s')).toBe('27,7777777778 m/s');
    expect(line('= 10 m/s em km/h')).toBe('36 km/h');
  });

  it('accepts the unit symbols as they are actually written', () => {
    expect(line('= 1 m² em cm²')).toBe('10000 cm²');
    expect(line('= 1 m2 em cm2')).toBe('10000 cm²');
    expect(line('= 0 °C em °F')).toBe('32 °F');
    expect(line('= 1000 mL em L')).toBe('1 L');
  });

  it('reads Portuguese names where the table lists them', () => {
    expect(line('= 10 quilometros em metros')).toBe('10000 m');
    expect(line('= 2 horas em minutos')).toBe('120 min');
    expect(line('= 1 litro em mililitros')).toBe('1000 mL');
  });
});

describe('every dimension converts', () => {
  it.each([
    ['= 1 km em m', '1000 m'],
    ['= 1000 m em km', '1 km'],
    ['= 100 cm em m', '1 m'],
    ['= 10 mm em cm', '1 cm'],
    ['= 1 mi em km', '1,609344 km'],
    ['= 12 in em cm', '30,48 cm'],
  ])('length: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 1000 g em kg', '1 kg'],
    ['= 1 kg em g', '1000 g'],
    ['= 1000000 mg em kg', '1 kg'],
    ['= 1 t em kg', '1000 kg'],
    ['= 1 lb em g', '453,59237 g'],
    ['= 16 oz em lb', '1 lb'],
  ])('mass: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 1000 mL em L', '1 L'],
    ['= 1 L em mL', '1000 mL'],
    ['= 1 m³ em L', '1000 L'],
    ['= 1 cm3 em mL', '1 mL'],
  ])('volume: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 0 C em F', '32 °F'],
    ['= 100 C em F', '212 °F'],
    ['= 32 F em C', '0 °C'],
    ['= 0 C em K', '273,15 K'],
    ['= -40 C em F', '-40 °F'],
    ['= 300 K em C', '26,85 °C'],
  ])('temperature: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 60 s em min', '1 min'],
    ['= 120 min em h', '2 h'],
    ['= 24 h em dias', '1 dia'],
    ['= 48 h em dias', '2 dias'],
    ['= 2 semanas em dias', '14 dias'],
    ['= 1500 ms em s', '1,5 s'],
  ])('time: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 1 m2 em cm2', '10000 cm²'],
    ['= 10000 cm2 em m2', '1 m²'],
    ['= 1 km2 em m2', '1000000 m²'],
    ['= 1 ha em m2', '10000 m²'],
  ])('area: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 1 GB em MB', '1000 MB'],
    ['= 1 GiB em MiB', '1024 MiB'],
    ['= 1 KB em B', '1000 B'],
    ['= 1 KiB em B', '1024 B'],
    ['= 1 TB em GB', '1000 GB'],
  ])('digital data: %s', (source, expected) => expect(line(source)).toBe(expected));

  it.each([
    ['= 100 km/h em m/s', '27,7777777778 m/s'],
    ['= 1 m/s em km/h', '3,6 km/h'],
    ['= 60 mph em km/h', '96,56064 km/h'],
  ])('speed: %s', (source, expected) => expect(line(source)).toBe(expected));
});

describe('the day and the week read as Portuguese', () => {
  it('uses the singular only for exactly one', () => {
    expect(line('= 24 h em dias')).toBe('1 dia');
    expect(line('= 36 h em dias')).toBe('1,5 dias');
    expect(line('= 12 h em dias')).toBe('0,5 dias');
    expect(line('= 7 dias em semanas')).toBe('1 semana');
    expect(line('= 14 dias em semanas')).toBe('2 semanas');
  });
});

describe('what a conversion refuses', () => {
  it('names an unknown unit on either side', () => {
    expect(line('= 10 foo em m')).toBe('!unknown-unit');
    expect(line('= 10 km em foo')).toBe('!unknown-unit');
    expect(line('= 10 banana em xicara')).toBe('!unknown-unit');
  });

  it('refuses two units that were never comparable', () => {
    expect(line('= 10 kg em km')).toBe('!incompatible-units');
    expect(line('= 1 L em m')).toBe('!incompatible-units');
    expect(line('= 10 km/h em m')).toBe('!incompatible-units');
    expect(line('= 1 m2 em m')).toBe('!incompatible-units');
  });

  it('refuses a temperature that cannot exist', () => {
    expect(line('= -300 C em K')).toBe('!invalid-conversion');
    expect(line('= -500 F em C')).toBe('!invalid-conversion');
    // ...and absolute zero itself is a reading, not an error.
    expect(line('= -273,15 C em K')).toBe('0 K');
  });

  it('reports the missing value before the missing unit', () => {
    expect(line('= banana km em m')).toBe('!unknown-variable');
    expect(shown(['x := 1', '= x km em m'])).toEqual([null, '1000 m']);
  });

  it('has no reading for a conversion with no target', () => {
    // `= 10 km` is trailing text the grammar has no rule for. Inventing a
    // target for it would be inventing the reader's intent.
    expect(line('= 10 km')).toBe('!invalid-expression');
    expect(line('= 10 km em')).toBe('!invalid-expression');
    expect(line('= 10 em m')).toBe('!invalid-expression');
    expect(line('= em')).toBe('!invalid-expression');
  });

  it('does not let a unit be declared into a variable', () => {
    // The known limitation of this phase, tested so it stays a known one.
    expect(shown(['distancia := 10 km', '= distancia em m'])).toEqual([
      '!invalid-expression',
      '!invalid-expression',
    ]);
  });
});

describe('conversions live beside everything the math engine already did', () => {
  it('does not disturb percentages, which still use "de"', () => {
    expect(
      shown(['= 10% de 200', '= 10 km em m', '= 200 + 10%', '= 200 - 10%']),
    ).toEqual(['20', '10000 m', '220', '180']);
  });

  it('leaves plain arithmetic, variables and aggregators exactly as they were', () => {
    expect(
      shown([
        '= 2 + 2',
        'preco := 120',
        'quantidade := 3',
        '= preco * quantidade',
        'Gastos:',
        '= 10',
        '= 20',
        '= 30',
        '= sum',
        '= avg',
        '= count',
      ]),
    ).toEqual([
      '4',
      null,
      null,
      '360',
      null,
      '10',
      '20',
      '30',
      '60',
      '20',
      '3',
    ]);
  });

  it('keeps a converted quantity out of an aggregated block', () => {
    // `sum`, `avg` and `count` add up plain numbers and know nothing about
    // units. A converted line ends the block rather than being totalled into
    // one, so nothing adds ten thousand metres to five of something else.
    expect(shown(['= 10', '= 20', '= 1 km em m', '= sum'])).toEqual([
      '10',
      '20',
      '1000 m',
      '0',
    ]);
    expect(shown(['= 1 km em m', '= 10', '= 20', '= sum'])).toEqual([
      '1000 m',
      '10',
      '20',
      '30',
    ]);
  });

  it('still refuses "de" for anything but a percentage', () => {
    expect(line('= 200 de 10')).toBe('!invalid-expression');
    expect(line('= 10 km de m')).toBe('!invalid-expression');
  });

  it('keeps "em" out of the variable namespace', () => {
    expect(shown(['em := 5'])).toEqual(['!invalid-name']);
    expect(line('= em + 1')).toBe('!invalid-expression');
  });
});

describe('conversions are reactive, because the note is re-evaluated whole', () => {
  it('follows the variable the conversion reads', () => {
    expect(shown(['distancia := 5', '= distancia km em m'])).toEqual([null, '5000 m']);
    expect(shown(['distancia := 10', '= distancia km em m'])).toEqual([null, '10000 m']);
  });

  it('invalidates a conversion whose variable went away, and recovers it', () => {
    expect(shown(['= distancia km em m'])).toEqual(['!unknown-variable']);
    expect(shown(['distancia := 3', '= distancia km em m'])).toEqual([null, '3000 m']);
  });
});

describe('a conversion is spelled with the engine own number format', () => {
  it('writes the decimal separator as a comma and groups nothing', () => {
    // The same rule the math engine has followed since it existed: `.` and `,`
    // are both read as decimal separators, so a grouped result would be one
    // this engine reads back as a different number.
    expect(line('= 1500 m em km')).toBe('1,5 km');
    expect(line('= 1 km2 em m2')).toBe('1000000 m²');
    expect(line('= 1 TB em B')).toBe('1000000000000 B');
  });

  it('hides the noise the conversion arithmetic leaves behind', () => {
    // `0 °C` reaches `31.999999999999943` through the kelvin, and is shown as
    // the 32 it is.
    expect(line('= 0 C em F')).toBe('32 °F');
    expect(line('= 100 C em F')).toBe('212 °F');
    expect(line('= 10000 cm2 em m2')).toBe('1 m²');
  });
});
