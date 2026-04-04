import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// Manages the rterm-agent process lifecycle.
///
/// In production, the agent binary is bundled as an asset and extracted to
/// app-private storage. For development, you can specify a custom binary
/// path and port override.
class AgentService {
  Process? _process;
  int? _port;

  /// The port the agent is listening on, or null if not running.
  int? get port => _port;

  /// Whether the agent process is currently running.
  bool get isRunning => _process != null;

  /// Start the rterm-agent process.
  ///
  /// [binaryPath] overrides the default binary location (for debug).
  /// [portOverride] if non-null, passes --port to the agent. Otherwise
  /// the agent picks a random port and prints the port number to stdout.
  ///
  /// Returns the port the agent is listening on.
  Future<int> start({String? binaryPath, int? portOverride}) async {
    if (_process != null) {
      throw StateError('Agent is already running on port $_port');
    }

    final binary = binaryPath ?? _defaultBinaryPath();
    final args = <String>[];
    if (portOverride != null) {
      args.addAll(['--port', portOverride.toString()]);
    }

    _process = await Process.start(binary, args);

    // Read stdout line by line looking for PORT=<n>
    final completer = Completer<int>();
    _process!.stdout
        .transform(const SystemEncoding().decoder)
        .transform(const LineSplitter())
        .listen((line) {
      if (!completer.isCompleted && line.startsWith('PORT=')) {
        final port = int.tryParse(line.substring(5));
        if (port != null) {
          completer.complete(port);
        }
      }
    });

    // If the process exits before we get a port, fail.
    _process!.exitCode.then((code) {
      if (!completer.isCompleted) {
        completer.completeError(
          StateError('Agent exited with code $code before reporting port'),
        );
      }
      _process = null;
      _port = null;
    });

    _port = await completer.future.timeout(
      const Duration(seconds: 10),
      onTimeout: () {
        _process?.kill();
        _process = null;
        throw TimeoutException('Agent did not report port within 10 seconds');
      },
    );
    return _port!;
  }

  /// Stop the agent process.
  Future<void> stop() async {
    _process?.kill();
    _process = null;
    _port = null;
  }

  String _defaultBinaryPath() {
    // In a real app this would resolve to the extracted asset path.
    // For now, assume it is on PATH or in a well-known location.
    return 'rterm-agent';
  }
}

/// A debug-mode agent reference that connects to an already-running agent
/// instead of spawning one.
class DebugAgentRef {
  final String host;
  final int port;

  const DebugAgentRef({this.host = '127.0.0.1', required this.port});
}
