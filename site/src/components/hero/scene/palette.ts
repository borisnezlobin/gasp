/** The scene's colours, from the site's light and dark tokens: paper, the
    note's line grey, and the whale's ink. */
export type ScenePalette = {
  paper: number;
  line: number;
  whaleBack: number;
  whaleBelly: number;
  shadow: number;
};

const LIGHT: ScenePalette = {
  paper: 0xf6f6f7,
  line: 0xd8d8dd,
  whaleBack: 0x2c2c31,
  whaleBelly: 0xeceae6,
  shadow: 0x18181b,
};

const DARK: ScenePalette = {
  paper: 0x151412,
  line: 0x34322e,
  whaleBack: 0xd9d4cb,
  whaleBelly: 0x5b5751,
  shadow: 0x000000,
};

export const paletteFor = (dark: boolean): ScenePalette => (dark ? DARK : LIGHT);
