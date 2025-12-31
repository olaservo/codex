# Codex MCP Client Compliance Report

**Date:** 2025-12-31
**Protocol Version:** 2025-06-18
**Test Framework:** MCP Conformance Tests v0.1.8

## Executive Summary

Codex implements an MCP client using the [rmcp](https://github.com/modelcontextprotocol/rust-sdk) Rust SDK (v0.12.0). Conformance testing confirms basic protocol compliance for initialization and tool operations. Elicitation capability is declared but not fully testable due to transport-level issues.

## Capability Declaration

Codex declares the following capabilities during initialization:

```rust
ClientCapabilities {
    experimental: None,
    roots: None,
    sampling: None,
    elicitation: Some(json!({})),  // Empty object = full support
}
```

| Capability | Declared | Implementation |
|------------|----------|----------------|
| **Elicitation** | Yes | Routes to TUI for user input |
| **Sampling** | No | Not supported |
| **Roots** | No | Not supported |

## Conformance Test Results

### Passing Tests

| Test | Status | Description |
|------|--------|-------------|
| `initialize` | **PASS** | MCP initialization handshake |
| `tools_call` | **PASS** | Tool listing and invocation |

### Failing/Blocked Tests

| Test | Status | Issue |
|------|--------|-------|
| `elicitation-sep1034-client-defaults` | **BLOCKED** | Transport issue - SSE stream not processing during tool call |
| `sse-retry` | **SKIPPED** | SSE retry behavior |
| `auth/*` | **SKIPPED** | OAuth flows (require browser) |

## Detailed Compliance Analysis

### 1. Protocol Handshake (COMPLIANT)

**Test:** `initialize`

Codex correctly implements the MCP initialization handshake:
- Sends `initialize` request with client info and capabilities
- Uses protocol version `2025-06-18`
- Receives and processes server capabilities
- Sends `initialized` notification

**Evidence:**
```
[mcp-client-initialization] SUCCESS Validates that MCP client properly initializes with server
```

### 2. Tool Operations (COMPLIANT)

**Test:** `tools_call`

Codex correctly implements tool operations:
- `tools/list` - Lists available tools from server
- `tools/call` - Invokes tools with arguments and receives results

**Evidence:**
```
[tool-add-numbers] SUCCESS Validates that the add_numbers tool works correctly
```

### 3. Elicitation (PARTIALLY COMPLIANT)

**Test:** `elicitation-sep1034-client-defaults`

**Declaration:** Codex declares `elicitation: {}` capability, indicating support for form-based user input requests.

**Implementation:**
- Codex has a full elicitation implementation that routes requests to the TUI
- `ElicitationRequestManager` handles incoming requests
- TUI displays forms and collects user responses

**Test Result:** BLOCKED

**Issue:** During conformance testing, the rmcp SDK's HTTP streaming transport doesn't process incoming SSE messages while awaiting a tool call response. The server sends an `elicitation/create` request via SSE during the tool call, but the client never receives it.

This is a transport-layer issue in the rmcp SDK, not a protocol implementation issue in Codex. The elicitation callback is correctly configured but never invoked because the SSE stream isn't being polled.

**Recommendation:**
1. Investigate rmcp SDK's bidirectional HTTP streaming behavior
2. Consider opening an issue on the rmcp repository
3. Test elicitation manually with real TUI interaction

### 4. Resources (FULLY IMPLEMENTED)

Codex fully supports MCP resources:

| Method | Status | Usage |
|--------|--------|-------|
| `resources/list` | ✅ Implemented | Used by `mcp_resource` tool handler |
| `resources/templates/list` | ✅ Implemented | Available in RmcpClient |
| `resources/read` | ✅ Implemented | Used to fetch resource content |

Resources are exposed to the AI agent via the `mcp__<server>__resource` tool pattern, allowing the agent to browse and read MCP resources from connected servers.

### 5. Prompts (NOT IMPLEMENTED)

Codex does **not** support MCP prompts as a client:

| Method | Status |
|--------|--------|
| `prompts/list` | ❌ Not implemented |
| `prompts/get` | ❌ Not implemented |

The `RmcpClient` wrapper does not expose prompt methods. This is a gap in Codex's MCP client implementation.

### 6. Server-to-Client Notifications (PARTIAL)

Codex receives notifications but **does not act on most of them**:

| Notification | Received | Action Taken |
|--------------|----------|--------------|
| `notifications/cancelled` | ✅ | ✅ Logged |
| `notifications/progress` | ✅ | ✅ Logged |
| `notifications/resources/updated` | ✅ | ⚠️ **Logged only** - no refresh |
| `notifications/resources/list_changed` | ✅ | ⚠️ **Logged only** - no refresh |
| `notifications/tools/list_changed` | ✅ | ⚠️ **Logged only** - no refresh |
| `notifications/prompts/list_changed` | ✅ | ⚠️ **Logged only** - no refresh |
| `logging` | ✅ | ✅ Logged at appropriate level |

**Gap:** When a server notifies that tools/resources have changed, Codex does not re-fetch the updated lists. Dynamic tool discovery is not supported - changes require connection restart.

### 7. Resource Subscriptions (NOT IMPLEMENTED)

| Method | Status |
|--------|--------|
| `resources/subscribe` | ❌ Not implemented |
| `resources/unsubscribe` | ❌ Not implemented |

Codex cannot subscribe to resource updates from MCP servers.

## Client Info

```
Name: codex-mcp-client
Version: 0.0.0 (development)
Title: Codex
```

## Transports Supported

| Transport | Supported | Notes |
|-----------|-----------|-------|
| **STDIO** | Yes | For local MCP servers |
| **HTTP Streaming** | Yes | For remote servers, with OAuth support |

## Known Limitations

1. **Prompts:** Not implemented - Codex cannot list or get prompts from MCP servers
2. **Elicitation in automated tests:** Cannot test elicitation without TUI due to transport behavior
3. **Sampling:** Not implemented - Codex cannot act as an LLM for servers
4. **Roots:** Not implemented - Codex doesn't expose filesystem roots to servers

## Recommendations

1. **For rmcp SDK:** Investigate why SSE stream isn't processed during request awaits
2. **For conformance framework:** Consider alternative test patterns for elicitation that don't require bidirectional communication during a single request
3. **For Codex:** Consider adding integration tests with real TUI for elicitation flows

## Test Commands

```bash
# Run conformance tests
codex mcp conformance <server-url>

# Using conformance framework
cd conformance
npm run start -- client \
  --command "codex mcp conformance" \
  --scenario initialize \
  --scenario tools_call
```

## Appendix: Code References

| Component | Location |
|-----------|----------|
| MCP Connection Manager | `core/src/mcp_connection_manager.rs` |
| RmcpClient wrapper | `rmcp-client/src/rmcp_client.rs` |
| Elicitation handler | `rmcp-client/src/logging_client_handler.rs` |
| Conformance command | `cli/src/mcp_cmd.rs` |
| Capability declaration | `core/src/mcp_connection_manager.rs:768-786` |
