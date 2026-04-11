import '../generated/rterm_rterm.protocol_generated.dart' as fb_gen;
import '../models/cell.dart';
import '../models/screen_buffer.dart';

/// Converts FlatBuffers-generated screen types to mobile model types.
/// Converts a FlatBuffers Cell to a mobile Cell model.
Cell cellFromFlatBuffer(fb_gen.Cell fbCell) {
  return Cell(
    ch: fbCell.ch,
    fg: CellColor(fbCell.fg),
    bg: CellColor(fbCell.bg),
    flags: fbCell.flags,
  );
}

/// Convert a FlatBuffers CursorState to mobile CursorState.
CursorState cursorStateFromFlatBuffer(fb_gen.CursorState fbCursor) {
  return CursorState(
    row: fbCursor.row,
    col: fbCursor.col,
    visible: fbCursor.visible,
    style: fbCursor.style,
  );
}

/// Convert a FlatBuffers CellRange to mobile CellRow.
CellRow cellRowFromFlatBuffer(fb_gen.CellRange range) {
  final cells = range.cells?.map(cellFromFlatBuffer).toList() ?? [];
  return CellRow(
    rowIndex: range.row,
    cells: cells,
  );
}

/// Convert a FlatBuffers ScreenSnapshot to mobile ScreenBuffer.
ScreenBuffer screenBufferFromSnapshot(fb_gen.ScreenSnapshot snapshot) {
  // Build rows from CellRanges
  final rows = <CellRow>[];

  if (snapshot.rows != null) {
    for (final range in snapshot.rows!) {
      rows.add(cellRowFromFlatBuffer(range));
    }
  }

  // Create the buffer
  final buffer = ScreenBuffer(
    cols: snapshot.cols,
    rows: snapshot.numRows,
    viewportOffset: snapshot.viewportOffset,
    buffer: rows,
    cursor: snapshot.cursor != null
        ? cursorStateFromFlatBuffer(snapshot.cursor!)
        : const CursorState(row: 0, col: 0, visible: true, style: 0),
    title: snapshot.title ?? '',
    altScreenActive: snapshot.altScreenActive,
    applicationCursorKeys: snapshot.applicationCursorKeys,
  );

  return buffer;
}

/// Apply a ScreenUpdate delta to an existing ScreenBuffer.
void applyScreenUpdate(ScreenBuffer screen, fb_gen.ScreenUpdate update) {
  // Update cursor if present
  if (update.cursor != null) {
    screen.cursor = cursorStateFromFlatBuffer(update.cursor!);
  }

  // Apply cell changes
  if (update.changes != null) {
    for (final range in update.changes!) {
      final row = range.row;
      final colStart = range.colStart;

      // Make sure row is in bounds
      if (row < 0 || row >= screen.rows) continue;

      // Get or create the row
      while (screen.buffer.length <= row) {
        // Need to add more rows (shouldn't happen with proper updates)
        screen.buffer.add(CellRow(
          rowIndex: screen.buffer.length,
          cells: List.filled(screen.cols, const Cell(
            ch: 0,
            fg: CellColor(CellColor.defaultColor),
            bg: CellColor(CellColor.defaultColor),
            flags: 0,
          )),
        ));
      }

      // Apply cells from this range
      if (range.cells != null) {
        for (int i = 0; i < range.cells!.length; i++) {
          final col = colStart + i;
          if (col < 0 || col >= screen.cols) continue;
          screen.buffer[row].cells[col] = cellFromFlatBuffer(range.cells![i]);
        }
      }
    }
  }

  // Update metadata
  screen.title = update.title ?? screen.title;
}
