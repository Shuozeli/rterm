import 'dart:js_interop';
import 'package:web/web.dart' as web;

void main() {
  print('[Demo] Starting...');

  // Create container
  final container = web.HTMLDivElement()
    ..style.width = '100%'
    ..style.height = '100%'
    ..style.display = 'flex'
    ..style.flexDirection = 'column';
  web.document.body!.append(container);

  // Create header
  final header = web.HTMLDivElement()
    ..style.padding = '8px'
    ..style.background = '#333'
    ..style.color = '#fff'
    ..style.fontFamily = 'sans-serif'
    ..style.fontSize = '14px';
  header.textContent = 'Canvas Demo';
  container.append(header);

  // Create canvas
  final canvas = web.HTMLCanvasElement()
    ..style.display = 'block'
    ..style.width = '100%'
    ..style.height = '100%';
  container.append(canvas);

  print('[Demo] Canvas created: ${canvas.clientWidth}x${canvas.clientHeight}');

  // Get context
  final ctx = canvas.getContext('2d') as web.CanvasRenderingContext2D;

  // Set canvas internal resolution
  canvas.width = 800;
  canvas.height = 600;

  print('[Demo] Canvas internal: ${canvas.width}x${canvas.height}');

  // Draw test pattern
  ctx.fillStyle = 'rgb(255,0,0)'.toJS;
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  ctx.fillStyle = 'rgb(0,255,0)'.toJS;
  ctx.fillRect(10, 10, 100, 100);

  ctx.fillStyle = 'rgb(0,0,255)'.toJS;
  ctx.fillRect(200, 200, 200, 200);

  ctx.fillStyle = 'rgb(255,255,0)'.toJS;
  ctx.font = 'bold 40px monospace';
  ctx.fillText('Hello World!', 50, 100);

  ctx.fillStyle = 'rgb(255,255,255)'.toJS;
  ctx.font = 'bold 24px monospace';
  ctx.fillText('Canvas Demo Works!', 50, 150);

  print('[Demo] Drawing complete');

  // Also verify body exists
  print('[Demo] Body: ${web.document.body}');
  print('[Demo] Done');
}
