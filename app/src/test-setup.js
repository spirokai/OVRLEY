/**
 * Vitest setup file — runs before each test suite.
 */

import '@testing-library/jest-dom/vitest'

if (typeof HTMLCanvasElement !== 'undefined') {
  HTMLCanvasElement.prototype.getContext = function () {
    return {
      font: '',
      measureText: () => ({
        width: 0,
        actualBoundingBoxAscent: 0,
        actualBoundingBoxDescent: 0,
        actualBoundingBoxLeft: 0,
        actualBoundingBoxRight: 0,
      }),
    }
  }
}
