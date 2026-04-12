import 'dart:async';
import 'dart:js_interop';
import 'dart:js_util' as js_util;
import 'dart:typed_data';
import 'package:flat_buffers/flat_buffers.dart' as fb;
import 'package:web/web.dart' as web;
import 'generated/rterm_rterm.protocol_generated.dart' as fb_gen;
import 'models.dart';

/// Strip 4-byte BE u32 length prefix from wire format.
Uint8List _stripLengthPrefix(Uint8List data) {
  if (data.length < 4) return data;
  final len = (data[0] << 24) | (data[1] << 16) | (data[2] << 8) | data[3];
  if (data.length < 4 + len) return data;
  return data.sublist(4, 4 + len);
}

/// Encode a message with 4-byte BE u32 length prefix (wire format).
Uint8List _encodeMessage(Uint8List payload) {
  final len = payload.length;
  final result = Uint8List(4 + len);
  result[0] = (len >> 24) & 0xFF;
  result[1] = (len >> 16) & 0xFF;
  result[2] = (len >> 8) & 0xFF;
  result[3] = len & 0xFF;
  result.setRange(4, 4 + len, payload);
  return result;
}

/// WebSocket client for connecting to rterm relay using FlatBuffers protocol.
class WebSocketClient {
  web.WebSocket? _ws;
  int _cols = 80;
  int _rows = 24;

  final _screenController = StreamController<ScreenBuffer>.broadcast();
  final _errorController = StreamController<String>.broadcast();

  Stream<ScreenBuffer> get onScreen => _screenController.stream;
  Stream<String> get onError => _errorController.stream;

  ScreenBuffer? _screen;
  ScreenBuffer? get screen => _screen;

  bool _connected = false;
  bool get connected => _connected;

  /// Connect to relay WebSocket server.
  Future<void> connect(String host, {int port = 4435}) async {
    final url = 'ws://$host:$port/ws';
    print('[WebSocket] Connecting to $url');

    _ws = web.WebSocket(url);
    // Request ArrayBuffer instead of Blob/TypedArray for binary data
    _ws!.binaryType = 'arraybuffer';

    _ws!.onOpen.listen((web.Event _) {
      print('[WebSocket] Connected');
      _connected = true;
      _sendResize();
    });

    _ws!.onMessage.listen((web.MessageEvent event) {
      final data = event.data;
      print('[WebSocket] onMessage, type=${data.runtimeType}');
      if (data is JSArrayBuffer) {
        print('[WebSocket] Handling as JSArrayBuffer');
        _handleBinaryMessage(Uint8List.view(data.toDart));
      } else if (data is String) {
        print('[WebSocket] Received text: $data');
      } else if (data is JSObject) {
        // Could be ArrayBufferView (Uint8Array etc) - try to get underlying buffer
        print('[WebSocket] Handling as JSObject');
        _tryHandleArrayBufferView(data);
      }
    });

    _ws!.onError.listen((web.Event error) {
      print('[WebSocket] Error: $error');
      _connected = false;
      _errorController.add('Connection error');
    });

    _ws!.onClose.listen((web.CloseEvent event) {
      print('[WebSocket] Closed: ${event.code} ${event.reason}');
      _connected = false;
    });

    await _waitForOpen();
  }

  Future<void> _waitForOpen() async {
    int retries = 0;
    while (_ws!.readyState != web.WebSocket.OPEN) {
      if (retries > 500) {
        throw Exception('WebSocket connection timeout');
      }
      await Future.delayed(const Duration(milliseconds: 10));
      retries++;
    }
  }

  Future<String> createSession({
    required String name,
    String shell = '/bin/bash',
    int cols = 80,
    int rows = 24,
  }) async {
    _cols = cols;
    _rows = rows;

    _sendCreateSession(name, shell, cols, rows);
    await Future.delayed(const Duration(milliseconds: 100));
    await _waitForScreen();
    return name;
  }

  Uint8List _buildClientMessage(fb_gen.ClientBodyTypeId type, fb.Packable body) {
    final builder = fb.Builder(deduplicateTables: false);
    final clientMessage = fb_gen.ClientMessageT(
      bodyType: type,
      body: body,
    );
    final offset = fb_gen.ClientMessage.pack(builder, clientMessage);
    builder.finish(offset);
    return builder.buffer;
  }

  void _sendCreateSession(String name, String shell, int cols, int rows) {
    if (_ws == null) return;

    final createSession = fb_gen.CreateSessionT(
      name: name,
      shell: shell,
      cols: cols,
      rows: rows,
    );

    final payload = _buildClientMessage(fb_gen.ClientBodyTypeId.CreateSession, createSession);
    _ws!.send(_encodeMessage(payload).toJS);
  }

  void _sendResize() {
    if (_ws == null) return;

    final resize = fb_gen.ResizeT(cols: _cols, rows: _rows);
    final payload = _buildClientMessage(fb_gen.ClientBodyTypeId.Resize, resize);
    _ws!.send(_encodeMessage(payload).toJS);
  }

  void sendKeys(List<int> data) {
    if (_ws == null) return;

    final keyInput = fb_gen.KeyInputT(data: data);
    final payload = _buildClientMessage(fb_gen.ClientBodyTypeId.KeyInput, keyInput);
    _ws!.send(_encodeMessage(payload).toJS);
  }

  void sendRawKeys(String keys) {
    sendKeys(keys.codeUnits);
  }

  void resize(int cols, int rows) {
    _cols = cols;
    _rows = rows;
    _sendResize();
  }

  void _handleBinaryMessage(Uint8List data) {
    try {
      // Strip length prefix (server uses same wire format)
      final payload = _stripLengthPrefix(data);

      final serverMsg = fb_gen.ServerMessage(payload);
      print('[WS] bodyType=${serverMsg.bodyType}');

      switch (serverMsg.bodyType?.value) {
        case 1: // ScreenUpdate
          final update = serverMsg.body as fb_gen.ScreenUpdate;
          print('[WS] ScreenUpdate');
          _handleScreenUpdate(update);
          break;
        case 2: // ScreenSnapshot
          final snapshot = serverMsg.body as fb_gen.ScreenSnapshot;
          print('[WS] ScreenSnapshot');
          _handleScreenSnapshot(snapshot);
          break;
        case 4: // Error
          final error = serverMsg.body as fb_gen.Error;
          _errorController.add(error.message ?? 'Unknown error');
          break;
        case 3: // Exit
          break;
        case 5: // SessionDetached
          break;
      }
    } catch (e) {
      print('[WebSocket] Parse error: $e');
    }
  }

  void _handleScreenSnapshot(fb_gen.ScreenSnapshot snapshot) {
    final cols = snapshot.cols;
    final numRows = snapshot.numRows;
    final screen = ScreenBuffer.empty(cols, numRows);

    final cursor = snapshot.cursor;
    if (cursor != null) {
      screen.cursorRow = cursor.row;
      screen.cursorCol = cursor.col;
      screen.cursorVisible = cursor.visible;
    }

    screen.altScreenActive = snapshot.altScreenActive;

    final rows = snapshot.rows;
    if (rows != null) {
      for (final rowRange in rows) {
        final rowIdx = rowRange.row;
        final cells = rowRange.cells;
        if (cells != null) {
          for (int i = 0; i < cells.length; i++) {
            if (i < screen.cols && rowIdx < screen.rows) {
              screen.setCell(rowIdx, i, _parseCell(cells[i]));
            }
          }
        }
      }
    }

    _screen = screen;
    _screenController.add(_screen!);
    print('[WS] ScreenSnapshot added to controller');
  }

  void _handleScreenUpdate(fb_gen.ScreenUpdate update) {
    print('[WS] _handleScreenUpdate called');
    try {
      if (_screen == null) {
        _screen = ScreenBuffer.empty(update.cols, update.rows);
      } else if (update.cols != _screen!.cols || update.rows != _screen!.rows) {
        _screen!.resize(update.cols, update.rows);
      }

      final changes = update.changes;
      if (changes != null) {
        for (final change in changes) {
          final rowIdx = change.row;
          final colStart = change.colStart;
          final cells = change.cells;
          if (cells != null) {
            for (int i = 0; i < cells.length; i++) {
              final colIdx = colStart + i;
              if (colIdx < _screen!.cols && rowIdx < _screen!.rows) {
                _screen!.setCell(rowIdx, colIdx, _parseCell(cells[i]));
              }
            }
          }
        }
      }

      final cursor = update.cursor;
      if (cursor != null) {
        _screen!.cursorRow = cursor.row;
        _screen!.cursorCol = cursor.col;
        _screen!.cursorVisible = cursor.visible;
      }

      _screenController.add(_screen!);
      print('[WS] ScreenUpdate added to controller');
    } catch (e) {
      print('[WS] Exception in _handleScreenUpdate: $e');
    }
  }

  Cell _parseCell(fb_gen.Cell cell) {
    final ch = cell.ch;
    String charStr;
    if (ch == 0 || ch == 32) {
      charStr = ' ';
    } else if (ch < 0x10000) {
      charStr = String.fromCharCode(ch);
    } else {
      charStr = String.fromCharCodes([(ch >> 10) + 0xD800, (ch & 0x3FF) + 0xDC00]);
    }

    // Fix alpha channel: if 0 (fully transparent), set to 0xFF (fully opaque)
    // The server sends RGB (0x00RRGGBB) but renderer expects ARGB (0xFFRRGGBB)
    int fixAlpha(int color) => (color >> 24) == 0 ? color | 0xFF000000 : color;

    return Cell(
      ch: charStr,
      fg: fixAlpha(cell.fg),
      bg: fixAlpha(cell.bg),
      flags: cell.flags,
    );
  }

  Future<void> _waitForScreen() async {
    if (_screen != null) return;
    final completer = Completer<void>();
    late StreamSubscription sub;
    sub = _screenController.stream.listen((_) {
      if (!completer.isCompleted) {
        completer.complete();
      }
      sub.cancel();
    });
    await completer.future.timeout(
      const Duration(seconds: 5),
      onTimeout: () {
        sub.cancel();
      },
    );
  }

  void disconnect() {
    _ws?.close();
    _connected = false;
  }

  void dispose() {
    disconnect();
    _screenController.close();
    _errorController.close();
  }

  void _tryHandleArrayBufferView(JSObject data) {
    try {
      // Try to get buffer property (works for ArrayBufferView types like Uint8Array)
      final buffer = js_util.getProperty<JSAny?>(data, 'buffer');

      if (buffer != null && buffer is JSArrayBuffer) {
        final byteLength = js_util.getProperty<int>(data, 'byteLength');
        final byteOffset = js_util.getProperty<int>(data, 'byteOffset') ?? 0;

        if (byteLength != null && byteOffset != null) {
          final dartBuffer = buffer.toDart;
          final bytes = Uint8List.view(dartBuffer, byteOffset, byteLength);
          _handleBinaryMessage(bytes);
          return;
        }
      }
    } catch (e) {
      print('[WebSocket] Inspection failed: $e');
    }
  }

  List<int> _jsObjectToList(JSObject obj, int length) {
    final result = <int>[];
    for (int i = 0; i < length; i++) {
      final val = js_util.getProperty(obj, i.toString());
      if (val is int) {
        result.add(val);
      }
    }
    return result;
  }
}
