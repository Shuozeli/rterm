import 'dart:math' as math;

import 'package:flutter/material.dart';
import '../models/screen_buffer.dart';

/// Configuration for terminal grid appearance.
class TerminalGridConfig {
  final Color defaultFg;
  final Color defaultBg;
  final double fontSize;
  final String fontFamily;
  final Color cursorColor;
  final Color selectionBg;

  const TerminalGridConfig({
    this.defaultFg = const Color(0xFFE5E5E5),
    this.defaultBg = const Color(0xFF1E1E1E),
    this.fontSize = 14.0,
    this.fontFamily = 'monospace',
    this.cursorColor = const Color(0xFFFFFFFF),
    this.selectionBg = const Color(0x40FFFFFF),
  });
}

/// Terminal grid widget that renders a ScreenBuffer.
class TerminalGrid extends StatelessWidget {
  final ScreenBuffer screen;
  final TerminalGridConfig config;
  final void Function(int row, int col)? onTap;
  final void Function(int row, int col)? onDoubleTap;
  final void Function(int row, int col, int button)? onMouseDown;
  final void Function(int row, int col, int button)? onMouseUp;
  final void Function(int row, int col, int button)? onMouseMove;

  const TerminalGrid({
    super.key,
    required this.screen,
    this.config = const TerminalGridConfig(),
    this.onTap,
    this.onDoubleTap,
    this.onMouseDown,
    this.onMouseUp,
    this.onMouseMove,
  });

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        return _TerminalGridRender(
          screen: screen,
          config: config,
          availableSize: constraints.biggest,
          onTap: onTap,
          onDoubleTap: onDoubleTap,
          onMouseDown: onMouseDown,
          onMouseUp: onMouseUp,
          onMouseMove: onMouseMove,
        );
      },
    );
  }
}

class _TerminalGridRender extends StatefulWidget {
  final ScreenBuffer screen;
  final TerminalGridConfig config;
  final Size availableSize;
  final void Function(int row, int col)? onTap;
  final void Function(int row, int col)? onDoubleTap;
  final void Function(int row, int col, int button)? onMouseDown;
  final void Function(int row, int col, int button)? onMouseUp;
  final void Function(int row, int col, int button)? onMouseMove;

  const _TerminalGridRender({
    required this.screen,
    required this.config,
    required this.availableSize,
    this.onTap,
    this.onDoubleTap,
    this.onMouseDown,
    this.onMouseUp,
    this.onMouseMove,
  });

  @override
  State<_TerminalGridRender> createState() => _TerminalGridRenderState();
}

class _TerminalGridRenderState extends State<_TerminalGridRender> {
  late Size _cellSize;
  late int _fitCols;
  late int _fitRows;
  int? _lastRow;
  int? _lastCol;

  @override
  void initState() {
    super.initState();
    _calculateLayout();
  }

  @override
  void didUpdateWidget(_TerminalGridRender oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.screen.cols != widget.screen.cols ||
        oldWidget.screen.rows != widget.screen.rows ||
        oldWidget.config.fontSize != widget.config.fontSize ||
        oldWidget.config.fontFamily != widget.config.fontFamily) {
      _calculateLayout();
    }
  }

  void _calculateLayout() {
    final fontSize = widget.config.fontSize;
    // Monospace: cell width ~ 0.5 * fontSize, height ~ 1.0 * fontSize
    _cellSize = Size(fontSize * 0.5, fontSize * 1.2);
    _fitCols = (widget.availableSize.width / _cellSize.width).floor().clamp(1, widget.screen.cols);
    _fitRows = (widget.availableSize.height / _cellSize.height).floor().clamp(1, widget.screen.rows);
  }

  (int, int) _positionToCell(Offset pos) {
    final col = (pos.dx / _cellSize.width).floor().clamp(0, _fitCols - 1);
    final row = (pos.dy / _cellSize.height).floor().clamp(0, _fitRows - 1);
    return (row, col);
  }

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTapDown: (details) {
        final (row, col) = _positionToCell(details.localPosition);
        widget.onTap?.call(row, col);
      },
      onPanUpdate: (details) {
        final (row, col) = _positionToCell(details.localPosition);
        if (row != _lastRow || col != _lastCol) {
          _lastRow = row;
          _lastCol = col;
          widget.onMouseMove?.call(row, col, 0);
        }
      },
      child: Container(
        color: widget.config.defaultBg,
        child: CustomPaint(
          size: Size(_fitCols * _cellSize.width, _fitRows * _cellSize.height),
          painter: _TerminalPainter(
            screen: widget.screen,
            config: widget.config,
            cellSize: _cellSize,
          ),
        ),
      ),
    );
  }
}

class _TerminalPainter extends CustomPainter {
  final ScreenBuffer screen;
  final TerminalGridConfig config;
  final Size cellSize;

  _TerminalPainter({
    required this.screen,
    required this.config,
    required this.cellSize,
  });

  @override
  void paint(Canvas canvas, Size size) {
    final fontStyle = FontStyle.normal;
    final boldFontWeight = FontWeight.bold;

    // Background
    canvas.drawRect(
      Rect.fromLTWH(0, 0, size.width, size.height),
      Paint()..color = config.defaultBg,
    );

    final cursor = screen.cursor;

    // Draw cells
    for (int row = 0; row < screen.rows && row < (size.height / cellSize.height).floor(); row++) {
      for (int col = 0; col < screen.cols && col < (size.width / cellSize.width).floor(); col++) {
        final cell = screen.cellAt(row, col);

        // Skip spacer cells for wide chars
        if (cell.isWideCharSpacer) continue;

        final x = col * cellSize.width;
        final y = row * cellSize.height;
        final cellRect = Rect.fromLTWH(x, y, cellSize.width, cellSize.height);

        // Determine effective colors (handle inverse)
        final fgColor = cell.effectiveFg(cell.isInverse).toColor();
        final bgColor = cell.effectiveBg(cell.isInverse).toColor();

        // Draw background if not default bg
        if (bgColor != config.defaultBg) {
          canvas.drawRect(cellRect, Paint()..color = bgColor);
        }

        // Apply dim to foreground
        Color effectiveFg = fgColor;
        if (cell.isDim) {
          effectiveFg = Color.fromARGB(
            (fgColor.a * 255.0).round(),
            (fgColor.r * 255.0 * 0.6).round(),
            (fgColor.g * 255.0 * 0.6).round(),
            (fgColor.b * 255.0 * 0.6).round(),
          );
        }

        // Hidden cells show as background only
        if (cell.isHidden) {
          continue;
        }

        // Draw character
        final char = cell.char;
        if (char != ' ' && char.isNotEmpty) {
          final textPainter = TextPainter(
            text: TextSpan(
              text: char,
              style: TextStyle(
                fontSize: config.fontSize,
                fontFamily: config.fontFamily,
                fontWeight: cell.isBold ? boldFontWeight : FontWeight.normal,
                fontStyle: cell.isItalic ? FontStyle.italic : fontStyle,
                color: effectiveFg,
              ),
            ),
            textDirection: TextDirection.ltr,
          );
          textPainter.layout();

          // Center text in cell
          final textX = x + (cellSize.width - textPainter.width) / 2;
          final textY = y + (cellSize.height - textPainter.height) / 2;

          textPainter.paint(canvas, Offset(textX, textY));
        }

        // Draw underlines
        if (cell.isUnderline || cell.isDoubleUnderline || cell.isDottedUnderline || cell.isDashedUnderline) {
          final linePaint = Paint()
            ..color = effectiveFg
            ..strokeWidth = 1.0;

          final yPos = y + cellSize.height - 2;
          if (cell.isUnderline) {
            canvas.drawLine(Offset(x + 1, yPos), Offset(x + cellSize.width - 1, yPos), linePaint);
          } else if (cell.isDoubleUnderline) {
            canvas.drawLine(Offset(x + 1, yPos - 2), Offset(x + cellSize.width - 1, yPos - 2), linePaint);
            canvas.drawLine(Offset(x + 1, yPos), Offset(x + cellSize.width - 1, yPos), linePaint);
          } else if (cell.isDashedUnderline) {
            _drawDashedLine(canvas, Offset(x + 1, yPos), Offset(x + cellSize.width - 1, yPos), linePaint);
          } else if (cell.isDottedUnderline) {
            _drawDottedLine(canvas, Offset(x + 1, yPos), Offset(x + cellSize.width - 1, yPos), linePaint);
          }
        }

        // Draw strikethrough
        if (cell.isStrikethrough) {
          final linePaint = Paint()
            ..color = effectiveFg
            ..strokeWidth = 1.0;
          final yPos = y + cellSize.height / 2;
          canvas.drawLine(Offset(x + 1, yPos), Offset(x + cellSize.width - 1, yPos), linePaint);
        }
      }
    }

    // Draw cursor
    if (cursor.visible &&
        cursor.row < (size.height / cellSize.height).floor() &&
        cursor.col < (size.width / cellSize.width).floor()) {
      final cursorX = cursor.col * cellSize.width;
      final cursorY = cursor.row * cellSize.height;
      final cursorRect = Rect.fromLTWH(cursorX, cursorY, cellSize.width, cellSize.height);

      canvas.drawRect(
        cursorRect,
        Paint()..color = config.cursorColor.withAlpha(180),
      );
    }
  }

  void _drawDashedLine(Canvas canvas, Offset start, Offset end, Paint paint) {
    const dashLength = 4.0;
    const gapLength = 2.0;
    final dx = end.dx - start.dx;
    final dy = end.dy - start.dy;
    final length = math.sqrt(dx * dx + dy * dy);
    if (length == 0) return;

    final unitDx = dx / length;
    final unitDy = dy / length;

    var current = 0.0;
    while (current < length) {
      final dashEnd = (current + dashLength).clamp(0, length);
      canvas.drawLine(
        Offset(start.dx + unitDx * current, start.dy + unitDy * current),
        Offset(start.dx + unitDx * dashEnd, start.dy + unitDy * dashEnd),
        paint,
      );
      current += dashLength + gapLength;
    }
  }

  void _drawDottedLine(Canvas canvas, Offset start, Offset end, Paint paint) {
    const dotLength = 2.0;
    const gapLength = 2.0;
    final dx = end.dx - start.dx;
    final dy = end.dy - start.dy;
    final length = math.sqrt(dx * dx + dy * dy);
    if (length == 0) return;

    final unitDx = dx / length;
    final unitDy = dy / length;

    var current = 0.0;
    while (current < length) {
      final dotEnd = (current + dotLength).clamp(0, length);
      canvas.drawLine(
        Offset(start.dx + unitDx * current, start.dy + unitDy * current),
        Offset(start.dx + unitDx * dotEnd, start.dy + unitDy * dotEnd),
        paint,
      );
      current += dotLength + gapLength;
    }
  }

  @override
  bool shouldRepaint(covariant _TerminalPainter oldDelegate) {
    return oldDelegate.screen != screen ||
        oldDelegate.config != config ||
        oldDelegate.cellSize != cellSize;
  }
}
