# gRPC & FlatBuffers Timeout 分析

<!-- agent-updated: 2026-04-05 -->

## gRPC Timeout 支持

### Server Side

**位置**: `grpc-server/src/server.rs`

```rust
pub struct Server {
    timeout: Option<Duration>,  // 默认: None (无时限)
    // ...
}

impl Server {
    /// 设置 per-request timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}
```

**实现** (`serve_connection`):
```rust
match timeout {
    Some(duration) => match tokio::time::timeout(duration, svc.call(req)).await {
        Ok(result) => result,
        Err(_elapsed) => {
            let status = grpc_core::Status::deadline_exceeded("request timed out");
            Ok(status.into_http())
        }
    },
    None => svc.call(req).await,
}
```

**gRPC Timeout Header**: `grpc-timeout` (标准 gRPC 头)

### Client Side

**位置**: `grpc-client/src/endpoint.rs`

```rust
pub struct Endpoint {
    timeout: Option<Duration>,          // per-request deadline
    connect_timeout: Option<Duration>,  // 连接建立超时
}

impl Endpoint {
    pub fn timeout(mut self, timeout: Duration) -> Self
    pub fn connect_timeout(mut self, timeout: Duration) -> Self
}
```

### 其他 Timeout 配置

- **HTTP/2 PING keep-alive**: `http2.keep_alive_timeout`
- **连接超时**: `connect_timeout`

---

## FlatBuffers Codec

**位置**: `grpc-codec-flatbuffers/src/lib.rs`

FlatBuffers codec 本身没有 timeout 相关逻辑，只负责 encode/decode。Timeout 由 gRPC 框架处理。

```rust
pub trait FlatBufferGrpcMessage: Send + Sync + Clone + 'static {
    fn encode_flatbuffer(&self) -> Vec<u8>;
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String>;
}
```

---

## 当前 rterm 使用情况

| 组件 | Timeout 设置 | 状态 |
|------|-------------|------|
| rterm-relay (Server) | 30s | ✅ 已实现 |
| rterm-agent (Server) | 30s | ✅ 已实现 |
| rterm-cli (Client) | 30s request, 10s connect | ✅ 已实现 |
| rterm-gui (Client) | 10s connect (H3) | ✅ 已实现 |

---

## 潜在问题

### 1. Server 无默认 Timeout

~~如果 RPC 请求处理卡住（如 PTY 挂起），server 会永远等待。~~ ✅ 已解决：Server 设置 30s timeout

**建议**: 为长时间运行的 RPC 设置合理 timeout：
- `session` (bidirectional streaming): 可能很长的生命周期，考虑不设 timeout 或很大的值
- `run_command`: 设置合理 timeout（如 30s - 5min）
- unary RPC: 设置短 timeout

### 2. 连接建立超时

~~Client 端 `connect_timeout` 未设置，连接可能无限等待。~~ ✅ 已解决：Client 设置 10s connect timeout

### 3. Flutter Client

Flutter gRPC 客户端使用 `grpc` package 的 timeout 机制，需要单独配置。

---

## 建议的 Timeout 配置

```rust
// Server 端
Server::builder()
    .timeout(Duration::from_secs(30))  // 默认 30s timeout
    // 或针对特定 RPC 在 handler 内部处理

// Client 端
Endpoint::from_share_channel(channel)
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(10))
```

---

## gRPC Timeout 最大值

gRPC timeout header (`grpc-timeout`) 最大值:
```rust
const MAX_GRPC_TIMEOUT_HOURS: u64 = 99_999_999;  // ~11,400 年
```
