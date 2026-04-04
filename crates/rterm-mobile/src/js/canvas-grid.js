// rterm mobile -- canvas-based terminal renderer
// Mirrors the Rust paint_grid() logic from rterm-wasm.
// Receives ScreenDataJson (from get_screen_snapshot) and renders to a canvas.

// Attribute bitflags (must match rterm-core::cell::Flags bit layout).
const ATTR_INVERSE = 0x0001;
const ATTR_BOLD = 0x0002;
const ATTR_ITALIC = 0x0004;
const ATTR_UNDERLINE = 0x0008;
const ATTR_WIDE = 0x0020;
const ATTR_WIDE_SPACER = 0x0040;
const ATTR_DIM = 0x0080;
const ATTR_HIDDEN = 0x0100;
const ATTR_STRIKEOUT = 0x0200;
const ATTR_DOUBLE_UNDERLINE = 0x0800;
const ATTR_UNDERCURL = 0x1000;
const ATTR_DOTTED_UNDERLINE = 0x2000;
const ATTR_DASHED_UNDERLINE = 0x4000;
const ATTR_ALL_UNDERLINES =
    ATTR_UNDERLINE | ATTR_DOUBLE_UNDERLINE | ATTR_UNDERCURL | ATTR_DOTTED_UNDERLINE | ATTR_DASHED_UNDERLINE;

const COLOR_DEFAULT = 0xFFFFFFFF;

// ANSI 16-color palette (indices 0–15).
const ANSI_COLORS = [
    [0, 0, 0],         // 0  black
    [205, 0, 0],      // 1  red
    [0, 205, 0],      // 2  green
    [205, 205, 0],    // 3  yellow
    [0, 0, 238],      // 4  blue
    [205, 0, 205],    // 5  magenta
    [0, 205, 205],    // 6  cyan
    [229, 229, 229],  // 7  white
    [127, 127, 127],  // 8  bright black
    [255, 0, 0],      // 9  bright red
    [0, 255, 0],      // 10 bright green
    [255, 255, 0],    // 11 bright yellow
    [92, 92, 255],    // 12 bright blue
    [255, 0, 255],    // 13 bright magenta
    [0, 255, 255],    // 14 bright cyan
    [255, 255, 255],  // 15 bright white
];

function unpackColor(packed, defaultR, defaultG, defaultB) {
    if (packed === COLOR_DEFAULT) {
        return [defaultR, defaultG, defaultB];
    }
    if ((packed >>> 24) === 0xFF) {
        // Indexed color.
        const idx = packed & 0xFF;
        if (idx < 16) {
            return ANSI_COLORS[idx];
        }
        if (idx < 232) {
            // 216-color cube.
            const n = idx - 16;
            const b = n % 6;
            const g = Math.floor((n / 6) % 6);
            const r = Math.floor(n / 36);
            const toVal = (v) => (v === 0 ? 0 : 55 + v * 40);
            return [toVal(r), toVal(g), toVal(b)];
        }
        // 24 grayscale.
        const v = 8 + (idx - 232) * 10;
        return [v, v, v];
    }
    // RGB.
    return [(packed >> 16) & 0xFF, (packed >> 8) & 0xFF, packed & 0xFF];
}

// TerminalRenderer: renders ScreenDataJson to a canvas element.
export class TerminalRenderer {
    constructor(canvas, options = {}) {
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.fontSize = options.fontSize || 14;
        this.fontFamily = options.fontFamily || 'Menlo, Consolas, "DejaVu Sans Mono", monospace';
        this.bgColor = [0, 0, 0];   // default terminal bg: black
        this.fgColor = [229, 229, 229]; // default terminal fg: white

        // Cache for measured cell size.
        this._cellSize = null; // { width, height }
        this._lastScreen = null;
    }

    _ensureCellSize() {
        if (this._cellSize) return this._cellSize;
        const ctx = this.ctx;
        ctx.font = `${this.fontSize}px ${this.fontFamily}`;
        const metrics = ctx.measureText('0'.repeat(20));
        const cellWidth = metrics.width / 20;
        const cellHeight = ctx.measureText('M').actualBoundingBoxAscent +
                           ctx.measureText('M').actualBoundingBoxDescent;
        this._cellSize = { width: Math.ceil(cellWidth), height: Math.ceil(cellHeight) + 1 };
        return this._cellSize;
    }

    _applyStylesFromCell(cell) {
        const ctx = this.ctx;
        let [fr, fg, fb] = unpackColor(cell.fg, this.fgColor[0], this.fgColor[1], this.fgColor[2]);
        let [br, bg, bb] = unpackColor(cell.bg, this.bgColor[0], this.bgColor[1], this.bgColor[2]);

        if (cell.flags & ATTR_INVERSE) {
            [fr, fg, fb, br, bg, bb] = [br, bg, bb, fr, fg, fb];
        }
        if (cell.flags & ATTR_DIM) {
            fr = Math.floor(fr * 0.6);
            fg = Math.floor(fg * 0.6);
            fb = Math.floor(fb * 0.6);
        }
        if (cell.flags & ATTR_HIDDEN) {
            [fr, fg, fb] = [br, bg, bb];
        }

        ctx.fillStyle = `rgb(${fr},${fg},${fb})`;
        ctx.strokeStyle = `rgb(${fr},${fg},${fb})`;
        ctx.globalAlpha = 1.0;
        return { fg: [fr, fg, fb], bg: [br, bg, bb] };
    }

    _drawCellText(x, y, ch, cellFlags, fgColor) {
        const ctx = this.ctx;
        ctx.fillText(ch, x, y + this.fontSize);
        // Faux bold: draw again with slight offset.
        if (cellFlags & ATTR_BOLD) {
            ctx.fillText(ch, x + 0.5, y + this.fontSize);
        }
    }

    _drawLine(x1, y1, x2, y2, stroke) {
        const ctx = this.ctx;
        ctx.beginPath();
        ctx.moveTo(x1, y1);
        ctx.lineTo(x2, y2);
        ctx.stroke();
    }

    _renderScreenData(sd) {
        const ctx = this.ctx;
        const { width: cellW, height: cellH } = this._ensureCellSize();
        const cols = sd.cols;
        const rows = sd.rows;

        // Clear canvas.
        ctx.fillStyle = `rgb(${this.bgColor[0]},${this.bgColor[1]},${this.bgColor[2]})`;
        ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);

        // Build a mutable cell grid initialized to empty.
        const defaultCell = { ch: ' ', fg: COLOR_DEFAULT, bg: COLOR_DEFAULT, flags: 0 };
        const grid = [];
        for (let r = 0; r < rows; r++) {
            grid.push(Array.from({ length: cols }, () => ({ ...defaultCell })));
        }

        // Apply cell changes.
        const changes = sd.changes || [];
        for (const cr of changes) {
            const row = cr.row;
            const colStart = cr.col_start;
            const cells = cr.cells || [];
            for (let i = 0; i < cells.length; i++) {
                const c = colStart + i;
                if (row < rows && c < cols) {
                    grid[row][c] = cells[i];
                }
            }
        }

        // Render each cell.
        for (let row = 0; row < rows; row++) {
            for (let col = 0; col < cols; col++) {
                const cell = grid[row][col];
                const x = col * cellW;
                const y = row * cellH;

                // Skip wide-char spacer cells (right half of CJK).
                if (cell.flags & ATTR_WIDE_SPACER) continue;

                const { fg, bg } = this._applyStylesFromCell(cell);

                // Background fill.
                if (cell.bg !== COLOR_DEFAULT) {
                    ctx.fillStyle = `rgb(${bg[0]},${bg[1]},${bg[2]})`;
                    ctx.fillRect(x, y, cellW, cellH);
                }

                // Skip plain spaces (but draw BG).
                if (cell.ch === ' ' && (cell.flags & ATTR_ALL_UNDERLINES) === 0 && (cell.flags & ATTR_STRIKEOUT) === 0) {
                    continue;
                }

                // Draw character.
                ctx.fillStyle = `rgb(${fg[0]},${fg[1]},${fg[2]})`;
                ctx.font = `${this.fontSize}px ${this.fontFamily}`;

                if (cell.flags & ATTR_WIDE) {
                    // Wide char: draw spanning 2 cells.
                    ctx.fillText(cell.ch, x, y + this.fontSize);
                    if (cell.flags & ATTR_BOLD) {
                        ctx.fillText(cell.ch, x + 0.5, y + this.fontSize);
                    }
                } else {
                    this._drawCellText(x, y, cell.ch, cell.flags, fg);
                }

                // Underline variants.
                if (cell.flags & ATTR_UNDERLINE) {
                    this._drawLine(x, y + cellH - 2, x + cellW, y + cellH - 2, ctx.strokeStyle);
                } else if (cell.flags & ATTR_DOUBLE_UNDERLINE) {
                    this._drawLine(x, y + cellH - 3, x + cellW, y + cellH - 3, ctx.strokeStyle);
                    this._drawLine(x, y + cellH - 1, x + cellW, y + cellH - 1, ctx.strokeStyle);
                } else if (cell.flags & (ATTR_UNDERCURL | ATTR_DOTTED_UNDERLINE | ATTR_DASHED_UNDERLINE)) {
                    this._drawLine(x, y + cellH - 2, x + cellW, y + cellH - 2, ctx.strokeStyle);
                }

                // Strikethrough.
                if (cell.flags & ATTR_STRIKEOUT) {
                    const ly = y + cellH / 2;
                    this._drawLine(x, ly, x + cellW, ly, ctx.strokeStyle);
                }
            }
        }

        // Draw cursor.
        if (sd.cursor_visible) {
            const cx = sd.cursor_col * cellW;
            const cy = sd.cursor_row * cellH;
            const cursorColor = [200, 200, 200];
            ctx.fillStyle = `rgba(${cursorColor[0]},${cursorColor[1]},${cursorColor[2]},0.8)`;

            switch (sd.cursor_style) {
                case 5: case 6:
                    // Bar cursor (thin vertical line).
                    ctx.fillRect(cx, cy, 2, cellH);
                    break;
                case 3: case 4:
                    // Underline cursor.
                    ctx.fillRect(cx, cy + cellH - 3, cellW, 3);
                    break;
                default:
                    // Block cursor (default).
                    ctx.fillRect(cx, cy, cellW, cellH);
                    break;
            }
        }
    }

    /**
     * Render a ScreenDataJson snapshot.
     * @param {Object} sd - ScreenDataJson from get_screen_snapshot.
     * @param {number} [cols] - Terminal column count (if not provided, uses sd.cols).
     * @param {number} [rows] - Terminal row count (if not provided, uses sd.rows).
     */
    render(sd, cols, rows) {
        // Resize canvas to fit the terminal grid.
        const { width: cellW, height: cellH } = this._ensureCellSize();
        const ncols = cols || sd.cols;
        const nrows = rows || sd.rows;
        this.canvas.width = ncols * cellW;
        this.canvas.height = nrows * cellH;
        this._renderScreenData(sd);
    }

    resizeFont(delta) {
        this.fontSize = Math.max(8, Math.min(32, this.fontSize + delta));
        this._cellSize = null; // Invalidate cache.
    }
}
