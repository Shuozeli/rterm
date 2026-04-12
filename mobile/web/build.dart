#!/usr/bin/env dart
import 'dart:io';
import 'dart:convert';

void main() async {
  // Compile Dart to JS
  print('Compiling Dart...');
  final compileResult = await Process.run('dart', ['compile', 'js', '-o', 'web/main.dart.js', 'lib/main.dart']);
  if (compileResult.exitCode != 0) {
    print('Compile failed: ${compileResult.stderr}');
    exit(1);
  }

  // Read compiled JS
  final jsFile = File('web/main.dart.js');
  final jsBytes = await jsFile.readAsBytes();

  // Compute content hash (first 16 chars of MD5)
  final hash = md5Hash(jsBytes);

  print('Hash: $hash');

  // Create hashed filename
  final hashedJs = 'web/main.dart.$hash.js';
  final hashedCss = 'web/main.dart.$hash.js.map';

  // Copy JS to hashed name
  await jsFile.copy(hashedJs);

  // Copy source map too
  final mapFile = File('web/main.dart.js.map');
  if (await mapFile.exists()) {
    await mapFile.copy(hashedCss);
  }

  // Update index.html
  final indexFile = File('web/index.html');
  var html = await indexFile.readAsString();
  // Replace any main.dart.HASH.js pattern
  html = html.replaceAll(RegExp(r'main\.dart\.[a-f0-9]+\.js'), 'main.dart.$hash.js');
  await indexFile.writeAsString(html);

  print('Built: main.dart.$hash.js');
}

/// Simple MD5 implementation for hashing
String md5Hash(List<int> bytes) {
  // MD5 initialization constants
  final a0 = 0x67452301;
  final b0 = 0xefcdab89;
  final c0 = 0x98badcfe;
  final d0 = 0x10325476;

  // Pre-processing: adding padding bits
  final originalLength = bytes.length;
  final bitLength = originalLength * 8;

  // Pad to 64 bytes (512 bits) aligned
  final paddedBytes = List<int>.from(bytes);
  paddedBytes.add(0x80);

  while ((paddedBytes.length % 64) != 56) {
    paddedBytes.add(0);
  }

  // Append original length in bits as 64-bit little-endian
  for (int i = 0; i < 8; i++) {
    paddedBytes.add((bitLength >> (i * 8)) & 0xff);
  }

  // Process each 64-byte chunk
  int a = a0, b = b0, c = c0, d = d0;

  for (int chunk = 0; chunk < paddedBytes.length; chunk += 64) {
    final m = List<int>.generate(16, (i) {
      int val = 0;
      for (int j = 0; j < 4; j++) {
        val |= paddedBytes[chunk + i * 4 + j] << (j * 8);
      }
      return val;
    });

    int f, g, temp;
    final r = [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
               5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
               4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
               6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21];

    for (int i = 0; i < 64; i++) {
      if (i < 16) {
        f = (b & c) | ((~b) & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | ((~d) & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | (~d));
        g = (7 * i) % 16;
      }

      temp = d;
      d = c;
      c = b;
      b = (b + (((a + f + m[g] + r[i]) & 0xffffffff) << r[(i ~/ 16) * 4 + i % 4])) & 0xffffffff;
      a = temp;
    }

    a = (a0 + a) & 0xffffffff;
    b = (b0 + b) & 0xffffffff;
    c = (c0 + c) & 0xffffffff;
    d = (d0 + d) & 0xffffffff;
  }

  // Convert to little-endian hex string
  String leHex(int n) {
    String s = '';
    for (int i = 0; i < 4; i++) {
      s += (n >> (i * 8) & 0xff).toRadixString(16).padLeft(2, '0');
    }
    return s;
  }

  final hashStr = leHex(a) + leHex(b) + leHex(c) + leHex(d);
  return hashStr.substring(0, 16);
}
