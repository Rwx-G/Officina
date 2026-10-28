// gifenc ships no types; this declares the three functions the README asset
// generator uses.
declare module "gifenc" {
  type Palette = number[][];
  interface Encoder {
    /** `palette` is required on the first frame, which makes it global;
     *  `dispose: 1` keeps the frame for the next to draw over. */
    writeFrame(
      index: Uint8Array,
      width: number,
      height: number,
      opts: { palette?: Palette; delay: number; transparent?: boolean; transparentIndex?: number; dispose?: number },
    ): void;
    finish(): void;
    bytes(): Uint8Array;
  }
  const gifenc: {
    GIFEncoder(): Encoder;
    quantize(rgba: Uint8Array, maxColors: number): Palette;
    applyPalette(rgba: Uint8Array, palette: Palette): Uint8Array;
  };
  export default gifenc;
}
