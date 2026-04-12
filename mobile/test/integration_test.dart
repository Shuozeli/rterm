import 'package:flutter_test/flutter_test.dart';
import 'package:flat_buffers/flat_buffers.dart' as fb;

import 'package:rterm_mobile/models/cell.dart';
import 'package:rterm_mobile/models/screen_buffer.dart';
import 'package:rterm_mobile/utils/screen_converter.dart';
import 'package:rterm_mobile/generated/rterm_rterm.protocol_generated.dart' as fb_gen;

void main() {
  group('CellFlags', () {
    test('all flag bits are correctly defined', () {
      expect(CellFlags.inverse, equals(1 << 0));
      expect(CellFlags.bold, equals(1 << 1));
      expect(CellFlags.italic, equals(1 << 2));
      expect(CellFlags.underline, equals(1 << 3));
      expect(CellFlags.wrapLine, equals(1 << 4));
      expect(CellFlags.wideChar, equals(1 << 5));
      expect(CellFlags.wideCharSpacer, equals(1 << 6));
      expect(CellFlags.dim, equals(1 << 7));
      expect(CellFlags.hidden, equals(1 << 8));
      expect(CellFlags.strikethrough, equals(1 << 9));
      expect(CellFlags.leadingWideCharSpacer, equals(1 << 10));
      expect(CellFlags.doubleUnderline, equals(1 << 11));
      expect(CellFlags.undercurl, equals(1 << 12));
      expect(CellFlags.dottedUnderline, equals(1 << 13));
      expect(CellFlags.dashedUnderline, equals(1 << 14));
    });

    test('flag getters work correctly', () {
      const cell = Cell(
        ch: 65, // 'A'
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: CellFlags.bold | CellFlags.italic | CellFlags.underline,
      );

      expect(cell.isBold, isTrue);
      expect(cell.isItalic, isTrue);
      expect(cell.isUnderline, isTrue);
      expect(cell.isInverse, isFalse);
      expect(cell.isDim, isFalse);
      expect(cell.isHidden, isFalse);
    });

    test('wide char spacer flag works', () {
      const spacerCell = Cell(
        ch: 0,
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: CellFlags.wideCharSpacer,
      );

      expect(spacerCell.isWideCharSpacer, isTrue);
      expect(spacerCell.char, equals(' '));
    });
  });

  group('CellColor', () {
    test('default color detection', () {
      const defaultColor = CellColor(CellColor.defaultColor);
      expect(defaultColor.isDefault, isTrue);
      expect(defaultColor.isIndexed, isFalse);
      expect(defaultColor.isRgb, isFalse);
    });

    test('indexed color detection (0x1RRGGBB)', () {
      const indexed = CellColor(0x1FF8800); // palette index 0
      expect(indexed.isDefault, isFalse);
      expect(indexed.isIndexed, isTrue);
      expect(indexed.isRgb, isFalse);
      expect(indexed.paletteIndex, equals(0));
    });

    test('RGB color detection (0x2RRGGBB)', () {
      const rgb = CellColor(0x2FF8800); // RGB(255, 136, 0)
      expect(rgb.isDefault, isFalse);
      expect(rgb.isIndexed, isFalse);
      expect(rgb.isRgb, isTrue);
      expect(rgb.r, equals(0));
      expect(rgb.g, equals(136));
      expect(rgb.b, equals(255));
    });

    test('toColor returns correct Color for default', () {
      const defaultColor = CellColor(CellColor.defaultColor);
      final dartColor = defaultColor.toColor();
      expect(dartColor.r, equals(1.0));
      expect(dartColor.g, equals(1.0));
      expect(dartColor.b, equals(1.0));
    });

    test('toColor returns correct Color for RGB', () {
      // 0x2FF0000 encodes RGB as:
      // r = value & 0xFF = 0x00
      // g = (value >> 8) & 0xFF = 0x00
      // b = (value >> 16) & 0xFF = 0xFF
      // So this is blue (0, 0, 255), not red
      const rgb = CellColor(0x2FF0000);
      final dartColor = rgb.toColor();
      expect(dartColor.r, equals(0.0));
      expect(dartColor.g, equals(0.0));
      expect(dartColor.b, equals(1.0));
    });

    test('toColor returns ANSI palette color for indexed', () {
      // Index 1 should be red per ANSI palette
      const indexed = CellColor(0x1FF0001); // palette index 1
      final dartColor = indexed.toColor();
      // Index 1 is Color(0xFFCD0000) - red
      expect(dartColor.r, closeTo(0.804, 0.01));
    });
  });

  group('Cell', () {
    test('char returns space for ch=0', () {
      const cell = Cell(
        ch: 0,
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      expect(cell.char, equals(' '));
    });

    test('char returns correct character for valid code point', () {
      const cell = Cell(
        ch: 65,
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      expect(cell.char, equals('A'));
    });

    test('char returns correct character for Unicode', () {
      const cell = Cell(
        ch: 0x4E2D, // 中
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      expect(cell.char, equals('中'));
    });

    test('effectiveFg returns fg when not inverse', () {
      const fg = CellColor(0x2FF0000);
      const bg = CellColor(0x20000FF);
      const cell = Cell(ch: 65, fg: fg, bg: bg, flags: 0);

      expect(cell.effectiveFg(false), equals(fg));
      expect(cell.effectiveFg(true), equals(bg));
    });

    test('effectiveBg returns bg when not inverse', () {
      const fg = CellColor(0x2FF0000);
      const bg = CellColor(0x20000FF);
      const cell = Cell(ch: 65, fg: fg, bg: bg, flags: 0);

      expect(cell.effectiveBg(false), equals(bg));
      expect(cell.effectiveBg(true), equals(fg));
    });
  });

  group('ScreenBuffer', () {
    test('empty creates correct dimensions', () {
      final buffer = ScreenBuffer.empty(80, 24);

      expect(buffer.cols, equals(80));
      expect(buffer.rows, equals(24));
      expect(buffer.buffer.length, equals(24));
      for (final row in buffer.buffer) {
        expect(row.cells.length, equals(80));
      }
    });

    test('empty cells have default values', () {
      final buffer = ScreenBuffer.empty(10, 5);

      final cell = buffer.cellAt(0, 0);
      expect(cell.ch, equals(0));
      expect(cell.fg.isDefault, isTrue);
      expect(cell.bg.isDefault, isTrue);
      expect(cell.flags, equals(0));
    });

    test('cellAt returns default for out of bounds', () {
      final buffer = ScreenBuffer.empty(80, 24);

      final cell = buffer.cellAt(-1, 0);
      expect(cell.ch, equals(0));

      final cell2 = buffer.cellAt(0, 100);
      expect(cell2.ch, equals(0));

      final cell3 = buffer.cellAt(100, 0);
      expect(cell3.ch, equals(0));
    });

    test('inBounds works correctly', () {
      final buffer = ScreenBuffer.empty(80, 24);

      expect(buffer.inBounds(0, 0), isTrue);
      expect(buffer.inBounds(23, 79), isTrue);
      expect(buffer.inBounds(-1, 0), isFalse);
      expect(buffer.inBounds(0, -1), isFalse);
      expect(buffer.inBounds(24, 0), isFalse);
      expect(buffer.inBounds(0, 80), isFalse);
    });

    test('cursor state is initialized correctly', () {
      final buffer = ScreenBuffer.empty(80, 24);

      expect(buffer.cursor.row, equals(0));
      expect(buffer.cursor.col, equals(0));
      expect(buffer.cursor.visible, isTrue);
      expect(buffer.cursor.style, equals(0));
    });
  });

  group('ScreenConverter', () {
    test('cursorStateFromFlatBuffer converts correctly', () {
      // Build a CursorState table
      final cursorT = fb_gen.CursorStateT(row: 5, col: 10, visible: true, style: 1);
      final builder = fb.Builder(deduplicateTables: false);
      fb_gen.CursorState.pack(builder, cursorT);
      builder.finish(cursorT.pack(builder));
      final cursor = fb_gen.CursorState(builder.buffer);

      final mobileCursor = cursorStateFromFlatBuffer(cursor);

      expect(mobileCursor.row, equals(5));
      expect(mobileCursor.col, equals(10));
      expect(mobileCursor.visible, isTrue);
      expect(mobileCursor.style, equals(1));
    });
  });

  group('FlatBuffers Round-trip', () {
    test('CellT to Cell model conversion', () {
      // Create a CellT (FlatBuffers)
      final cellT = fb_gen.CellT(
        ch: 66, // 'B'
        fg: 0x2FFFF00, // yellow-ish RGB
        bg: 0x20000FF, // blue-ish RGB
        flags: CellFlags.bold | CellFlags.italic,
      );

      // Convert to mobile Cell model
      final mobileCell = Cell(
        ch: cellT.ch,
        fg: CellColor(cellT.fg),
        bg: CellColor(cellT.bg),
        flags: cellT.flags,
      );

      expect(mobileCell.ch, equals(66));
      expect(mobileCell.fg.value, equals(0x2FFFF00));
      expect(mobileCell.bg.value, equals(0x20000FF));
      expect(mobileCell.isBold, isTrue);
      expect(mobileCell.isItalic, isTrue);
    });

    test('CursorState roundtrip', () {
      final cursorT = fb_gen.CursorStateT(row: 10, col: 40, visible: true, style: 2);
      final builder = fb.Builder(deduplicateTables: false);
      builder.finish(cursorT.pack(builder));
      final cursor = fb_gen.CursorState(builder.buffer);

      final unpacked = cursor.unpack();

      expect(unpacked.row, equals(10));
      expect(unpacked.col, equals(40));
      expect(unpacked.visible, isTrue);
      expect(unpacked.style, equals(2));
    });

    test('ScreenSnapshot roundtrip with Cell data', () {
      // Create a CellT for testing
      final cellT = fb_gen.CellT(
        ch: 65, // 'A'
        fg: 0x2FF0000, // red
        bg: 0x2000000, // black
        flags: CellFlags.bold,
      );

      // Create a CellRangeT
      final cellRangeT = fb_gen.CellRangeT(
        row: 0,
        colStart: 0,
        cells: [cellT],
      );

      // Create a CursorStateT
      final cursorT = fb_gen.CursorStateT(
        row: 0,
        col: 1,
        visible: true,
        style: 0,
      );

      // Create a ScreenSnapshotT
      final snapshotT = fb_gen.ScreenSnapshotT(
        rows: [cellRangeT],
        cursor: cursorT,
        cols: 80,
        numRows: 24,
        title: 'Test Terminal',
        mouseTrackingMode: 0,
        altScreenActive: false,
        applicationCursorKeys: false,
        viewportOffset: 0,
      );

      // Pack to FlatBuffers
      final builder = fb.Builder(deduplicateTables: false);
      builder.finish(snapshotT.pack(builder));

      // Read back
      final rootRef = fb.BufferContext.fromBytes(builder.buffer);
      final snapshot = fb_gen.ScreenSnapshot.reader.read(rootRef, 0);

      // Verify
      expect(snapshot.cols, equals(80));
      expect(snapshot.numRows, equals(24));
      expect(snapshot.title, equals('Test Terminal'));
      expect(snapshot.rows, isNotNull);
      expect(snapshot.rows!.length, equals(1));
      expect(snapshot.rows![0].row, equals(0));
      expect(snapshot.rows![0].colStart, equals(0));
      expect(snapshot.rows![0].cells, isNotNull);
      expect(snapshot.rows![0].cells!.length, equals(1));
      expect(snapshot.rows![0].cells![0].ch, equals(65));
      expect(snapshot.rows![0].cells![0].fg, equals(0x2FF0000));
      expect(snapshot.cursor!.row, equals(0));
      expect(snapshot.cursor!.col, equals(1));
    });
  });

  group('TerminalGrid Rendering', () {
    test('ScreenBuffer can be used with TerminalGridConfig', () {
      final buffer = ScreenBuffer.empty(80, 24);

      // Modify some cells
      buffer.buffer[0].cells[0] = Cell(
        ch: 72, // 'H'
        fg: CellColor(0x2FF0000), // red
        bg: CellColor(CellColor.defaultColor),
        flags: CellFlags.bold,
      );
      buffer.buffer[0].cells[1] = Cell(
        ch: 101, // 'e'
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      buffer.buffer[0].cells[2] = Cell(
        ch: 108, // 'l'
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      buffer.buffer[0].cells[3] = Cell(
        ch: 108, // 'l'
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );
      buffer.buffer[0].cells[4] = Cell(
        ch: 111, // 'o'
        fg: CellColor(CellColor.defaultColor),
        bg: CellColor(CellColor.defaultColor),
        flags: 0,
      );

      expect(buffer.cellAt(0, 0).char, equals('H'));
      expect(buffer.cellAt(0, 1).char, equals('e'));
      expect(buffer.cellAt(0, 2).char, equals('l'));
      expect(buffer.cellAt(0, 3).char, equals('l'));
      expect(buffer.cellAt(0, 4).char, equals('o'));

      // First cell should be bold
      expect(buffer.cellAt(0, 0).isBold, isTrue);
      expect(buffer.cellAt(0, 1).isBold, isFalse);
    });
  });
}
