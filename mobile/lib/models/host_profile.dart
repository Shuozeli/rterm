import 'package:uuid/uuid.dart';

class HostProfile {
  final String id;
  String name;
  String hostname;
  int port;
  String username;
  String authType; // 'password' or 'key'
  String? password;
  String? privateKey;
  /// Relay server URL (e.g. 'https://relay.example.com:4433').
  /// If null, uses the global relay_url from app settings.
  String? relayUrl;

  HostProfile({
    String? id,
    required this.name,
    required this.hostname,
    this.port = 22,
    this.username = 'root',
    this.authType = 'password',
    this.password,
    this.privateKey,
    this.relayUrl,
  }) : id = id ?? const Uuid().v4();

  Map<String, dynamic> toJson() => {
        'id': id,
        'name': name,
        'hostname': hostname,
        'port': port,
        'username': username,
        'authType': authType,
        'password': password,
        'privateKey': privateKey,
        'relayUrl': relayUrl,
      };

  factory HostProfile.fromJson(Map<String, dynamic> json) => HostProfile(
        id: json['id'] as String,
        name: json['name'] as String,
        hostname: json['hostname'] as String,
        port: json['port'] as int? ?? 22,
        username: json['username'] as String? ?? 'root',
        authType: json['authType'] as String? ?? 'password',
        password: json['password'] as String?,
        privateKey: json['privateKey'] as String?,
        relayUrl: json['relayUrl'] as String?,
      );

  HostProfile copyWith({
    String? name,
    String? hostname,
    int? port,
    String? username,
    String? authType,
    String? password,
    String? privateKey,
    String? relayUrl,
  }) =>
      HostProfile(
        id: id,
        name: name ?? this.name,
        hostname: hostname ?? this.hostname,
        port: port ?? this.port,
        username: username ?? this.username,
        authType: authType ?? this.authType,
        password: password ?? this.password,
        privateKey: privateKey ?? this.privateKey,
        relayUrl: relayUrl ?? this.relayUrl,
      );
}
