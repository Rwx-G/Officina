// What every view that acts on one click shares (AD-35, AD-55, D-28).

/** True for a click made with a pointer. Enter and Space on a focused button
 *  also fire `click`, with `detail` 0: no key approves or confirms. */
export function isPointerClick(event: MouseEvent): boolean {
  return event.detail > 0;
}
