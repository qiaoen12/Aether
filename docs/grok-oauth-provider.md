# Grok OAuth 订阅账号提供商 — 变更记录

## 背景

Aether 原有 `grok` 提供商对接 `https://grok.com` 网页版逆向接口（Cookie SSO）。本次新增 `grok_oauth` 提供商类型，参考 [sub2api](https://github.com/Wei-Shaw/sub2api) 的 Grok OAuth 订阅账号实现，对接 `https://cli-chat-proxy.grok.com/v1` 的 OpenAI Responses 兼容代理。

### 三种 Grok 接入形态对照

| 提供商类型 | Base URL | 性质 | 认证方式 |
|---|---|---|---|
| `grok`（原有） | `https://grok.com` | 网页版逆向 | Cookie SSO（`sso`/`sso-rw`/`cf_clearance`） |
| `grok_oauth`（新增） | `https://cli-chat-proxy.grok.com/v1` | Grok Build/CLI 订阅代理 | xAI OAuth2 PKCE + Bearer Token |
| `custom`（已有） | `https://api.x.ai/v1` | xAI 官方付费 API | API Key |

## 设计决策

`grok_oauth` 架构上**接近 Codex**（OAuth Bearer + OpenAI Responses 格式 + 标准传输路径），而非原有 `grok`（Cookie SSO + 逆向工程 + 自定义运行时）。

- 复用 Aether 现有的 `same_format_provider` 标准传输路径，无需编写自定义运行时
- 复用 `GenericOAuthRefreshAdapter` 做 token 自动刷新
- 复用 `openai:responses` 格式的 URL 构建（`{base_url}/responses`）和计费规则
- OAuth 流程复用 `GenericProviderOAuthTemplate` 注册表，PKCE + form-encoded token 交换

### xAI OAuth 客户端参数（来自 sub2api）

| 参数 | 值 |
|---|---|
| Authorize URL | `https://auth.x.ai/oauth2/authorize` |
| Token URL | `https://auth.x.ai/oauth2/token` |
| Client ID | `b1a00492-073a-47ea-816f-4c329264a828` |
| Scopes | `openid profile email offline_access grok-cli:access api:access` |
| Redirect URI | `http://127.0.0.1:56121/callback` |
| PKCE | S256 |
| Client Secret | 无（公共 PKCE 客户端） |

### CLI 网关必需头

`cli-chat-proxy.grok.com` 要求 OAuth 请求携带 CLI 身份头，否则拒绝。在 `enrich_grok_oauth_identity` 中自动注入到 `auth_config.headers`：

| 头 | 值 |
|---|---|
| `User-Agent` | `aether-grok-oauth/1.0` |
| `X-Grok-Client-Version` | `0.2.93` |

---

## 已完成的改动

### 后端（Rust）

#### 1. `crates/aether-provider/transport/src/provider_types.rs`

- 新增 `GROK_OAUTH_RUNTIME_POLICY`：`fixed_provider=true`、`api_format_inheritance=OAuth`、`oauth_is_bearer_like=true`、`supports_model_fetch=false`、`supports_local_openai_chat_transport=false`
- 新增 `GROK_OAUTH_FIXED_PROVIDER_TEMPLATE`：`provider_type="grok_oauth"`、`base_url="https://cli-chat-proxy.grok.com/v1"`、端点 `openai:responses`（force_stream）+ `openai:chat`
- `fixed_provider_template()` 新增 `"grok_oauth"` match arm
- `provider_type_admin_oauth_template()` 新增 `"grok_oauth"` OAuth 模板（xAI PKCE 流程）
- `ADMIN_PROVIDER_OAUTH_TEMPLATE_TYPES` 新增 `"grok_oauth"`
- 新增 3 个单元测试：模板端点、OAuth 模板参数、admin OAuth 模板广播

#### 2. `apps/aether-gateway/src/handlers/admin/provider/write/normalize.rs`

- `normalize_provider_type_input()` 新增 `"grok_oauth"` 到允许列表
- 错误消息更新为包含 `grok_oauth`
- 新增 1 个单元测试：`normalize_provider_type_supports_grok_oauth`

#### 3. `apps/aether-gateway/src/provider_key_auth.rs`

- `provider_uses_bearer_oauth_runtime()` 新增 `"grok_oauth"`，使 OAuth key 被识别为 Bearer 运行时认证
- 这使 `grok_oauth` 的 OAuth key 自动获得 `can_refresh_oauth=true`、`credential_kind=OAuthSession`、`runtime_auth_kind=Bearer`
- 新增 1 个单元测试：`recognizes_grok_oauth_as_bearer_runtime`

#### 4. `crates/aether-oauth/src/provider/providers/generic.rs`

- `GENERIC_PROVIDER_OAUTH_TEMPLATES` 新增 `grok_oauth` 条目（xAI OAuth2 PKCE，form-encoded）
- 新增 `enrich_grok_oauth_identity()` 函数：
  - 自动注入 CLI 网关必需头（`User-Agent`、`X-Grok-Client-Version`）到 `auth_config.headers`
  - 从 access_token / id_token 的 JWT claims 提取 `email`、`sub`、`team_id`
- `enrich_generic_identity()` 新增 `grok_oauth` 分支调用
- 新增 2 个单元测试：`grok_oauth_template_uses_xai_pkce_flow`、`grok_oauth_identity_injects_cli_headers_and_jwt_claims`
- 更新 `resolves_generic_provider_templates` 测试断言 `grok_oauth` 存在

### 前端（Vue/TypeScript）

#### 5. `frontend/src/api/endpoints/types/provider.ts`

- `ProviderType` union 新增 `'grok_oauth'`

#### 6. `frontend/src/features/providers/components/ProviderFormDialog.vue`

- 新建模式和编辑模式的 `<SelectItem>` 下拉均新增 `grok_oauth` → `Grok OAuth`

#### 7. `frontend/src/features/providers/components/PriorityManagementDialog.vue`

- `PROVIDER_TYPE_LABELS` 新增 `grok_oauth: 'Grok OAuth'`，并补全了原有缺失的 `windsurf`

#### 8. `frontend/src/utils/oauth-icons.ts`

- `OAUTH_ICONS` 新增 `grok_oauth`（复用 grok 的 SVG 图标）

---

## 验证结果

| 检查项 | 结果 |
|---|---|
| `cargo fmt --check` | 通过 |
| `cargo check -p aether-provider-transport` | 通过 |
| `cargo check -p aether-oauth` | 通过 |
| `cargo check -p aether-gateway` | 通过 |
| `cargo test -p aether-provider-transport --lib provider_types` | 15 passed |
| `cargo test -p aether-oauth --lib generic` | 6 passed |
| `cargo test -p aether-gateway --lib normalize_provider_type` | 3 passed |
| `cargo test -p aether-gateway --lib recognizes_grok_oauth` | 2 passed |

---

## 运行时行为说明

### 请求转发路径

1. 客户端发送 OpenAI Responses / Chat 格式请求到 Aether
2. `grok_oauth` 不匹配 `is_grok_plan()`（只匹配 `grok`），走标准 `execute_direct_sync_runtime` / stream 路径
3. `same_format_provider` 标准路径构建 URL：`build_openai_responses_url()` → `https://cli-chat-proxy.grok.com/v1/responses`
4. `GenericOAuthRefreshAdapter` 解析 Bearer token 并注入 `Authorization` 头
5. `auth_config.headers` 中的 CLI 网关头通过 `apply_local_auth_config_header_overrides` 自动注入

### Token 刷新

- `GenericOAuthRefreshAdapter` 在 token 过期前 120 秒自动刷新
- 刷新请求：`POST https://auth.x.ai/oauth2/token`，form-encoded `grant_type=refresh_token` + `client_id` + `refresh_token`
- 刷新响应若省略 `refresh_token`，保留旧值（`GenericProviderOAuthAdapter::refresh` 已有此逻辑）
- 管理后台可手动触发 OAuth 刷新

### 不需要改动的部分（自动复用）

- **数据库 schema**：`provider_type` 列是 `TEXT` 无 CHECK 约束，无需迁移
- **URL 构建**：`build_openai_responses_url()` 已构建 `{base_url}/responses`
- **计费**：基于 `api_format`（`openai:responses`），非 `provider_type`，自动适用
- **端点模板调和**：`reconcile_admin_fixed_provider_template_endpoints()` 由注册表驱动，自动创建端点记录

---

## 待办 / 后续工作

### 部署相关

- [ ] 在 `vps3` 构建派生镜像（基于 `0.7.10` 或更新版本），包含本次后端 + 前端改动
- [ ] 更新 `/opt/aether/compose/.env` 的 `APP_IMAGE` 指向新镜像，保留回退路径
- [ ] 部署后验证 `/readyz` 和容器健康
- [ ] 验证管理后台可创建 `grok_oauth` 类型提供商并完成 OAuth 绑定

### 功能增强（可选，未实现）

- [ ] **媒体端点支持**：sub2api 支持 `/v1/images/generations`、`/v1/videos/generations` 等媒体端点转发到 `cli-chat-proxy.grok.com/v1`。当前实现只注册了 `openai:responses` 和 `openai:chat`，媒体端点未包含。
- [ ] **计费探针 / 媒体资格检查**：sub2api 通过 `GET /billing?format=credits` 探测账号媒体生成资格，403 标记为不可用。Aether 暂未实现此探针。
- [ ] **SSO Cookie → OAuth 转换**：sub2api 支持用 Grok 网页版 SSO Cookie 通过 device code flow 换取 OAuth token。Aether 暂未实现此路径。
- [ ] **Chat → Responses 桥接**：sub2api 将简单的 Chat Completions 请求转换为 Responses 格式发送。Aether 的格式转换引擎已支持此能力，但 `grok_oauth` 模板中 `openai:chat` 端点直接转发 chat 格式，未强制桥接到 responses。
- [ ] **xAI API Key 提供商类型**：目前 xAI API Key 账号可通过 `custom` 类型 + `base_url=https://api.x.ai/v1` 实现。如果需要独立类型（如 `grok_apikey`），可另行注册。
- [ ] **前端 OAuthAccountDialog 适配**：`grok_oauth` 默认走标准 OAuth 模式（非 import 模式），当前逻辑正确，但可能需要针对 Grok OAuth 定制说明文案。

### 源码同步

- [ ] 将改动提交到合适的分支或 fork，供 `vps3` 构建派生镜像使用
- [ ] 考虑向上游 `fawney19/Aether` 提交 PR（如果上游接受此类贡献）
