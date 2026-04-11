import 'dart:convert';
import 'package:shared_preferences/shared_preferences.dart';
import '../models/host_profile.dart';

class HostStorage {
  static const _key = 'host_profiles';
  static const _settingsKey = 'app_settings';
  static const _initializedKey = 'hosts_initialized';

  Future<List<HostProfile>> loadAll() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_key);
    if (raw == null) return [];
    final list = jsonDecode(raw) as List<dynamic>;
    return list
        .map((e) => HostProfile.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  Future<void> save(HostProfile profile) async {
    final profiles = await loadAll();
    final idx = profiles.indexWhere((p) => p.id == profile.id);
    if (idx >= 0) {
      profiles[idx] = profile;
    } else {
      profiles.add(profile);
    }
    await _persist(profiles);
  }

  Future<void> delete(String id) async {
    final profiles = await loadAll();
    profiles.removeWhere((p) => p.id == id);
    await _persist(profiles);
  }

  Future<void> _persist(List<HostProfile> profiles) async {
    final prefs = await SharedPreferences.getInstance();
    final raw = jsonEncode(profiles.map((p) => p.toJson()).toList());
    await prefs.setString(_key, raw);
  }

  /// Initialize default hosts for testing if not already done.
  Future<void> initializeDefaultHosts() async {
    final prefs = await SharedPreferences.getInstance();
    if (prefs.getBool(_initializedKey) == true) return;

    final defaultHosts = [
      HostProfile(
        name: 'This Server',
        hostname: '100.95.116.72',
        port: 22,
        username: 'cyuan',
        authType: 'password',
        password: 'cyuan',
      ),
    ];

    await _persist(defaultHosts);
    await prefs.setBool(_initializedKey, true);
  }

  /// Load app settings (e.g., relay_url).
  Future<Map<String, String>> loadSettings() async {
    final prefs = await SharedPreferences.getInstance();
    final raw = prefs.getString(_settingsKey);
    if (raw == null) return {};
    return Map<String, String>.from(jsonDecode(raw) as Map);
  }

  /// Save app settings.
  Future<void> saveSettings(Map<String, String> settings) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_settingsKey, jsonEncode(settings));
  }
}
