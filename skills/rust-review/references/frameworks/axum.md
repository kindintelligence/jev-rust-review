# Axum profile (checked against axum 0.8.9 docs)

Load this only when the project facts list `axum`.

## Extractors

- Extractors run left to right. The body can be consumed only once. So the body extractor (`Json`, `Form`, `String`, `Bytes`, `Request`) must be the **last** argument. Only it may implement `FromRequest`; the others must implement `FromRequestParts`. **The compiler enforces this**: the handler does not implement `Handler` otherwise. So do not report it; `cargo check` does.
- When an extractor fails, its rejection *is* the response and the handler never runs. The default rejections are plain-text 4xx responses. APIs that promise JSON errors need `Result<Json<T>, JsonRejection>` or a custom extractor that wraps the built-in one.
- `Option<T>` extractors only work where `OptionalFromRequestParts`/`OptionalFromRequest` is implemented (0.8). They give `None` for *missing* data but still reject *malformed* data.

## State

- `State<T>` with `.with_state()` is checked at compile time. `Extension<T>` fails at **runtime with a 500** when the layer that inserts it is missing for a route.
- `Router<S>` means "still needs state `S`"; `.with_state(s)` produces `Router<()>`.
- State is cloned per request: keep it cheap to clone, with `Arc` inside.

## Middleware order

- With repeated `Router::layer` calls, **the layer added last runs first** on the request (onion model).
- With `tower::ServiceBuilder`, layers run **top to bottom**. The docs recommend `ServiceBuilder` for multiple layers because the order reads naturally.
- `route_layer` applies only to matched routes, not the fallback. Use it for auth that should still let 404s through.
- Fallible middleware needs `HandleErrorLayer`, because services must have `Infallible` errors.
- Check that:
  - authentication runs before handlers that need it;
  - timeouts wrap what they should;
  - CORS sees preflight requests.

## Responses and errors

- Handlers return `impl IntoResponse`, often `Result<T, AppError>` where `AppError: IntoResponse`. A `From<anyhow::Error>` impl lets `?` work.
- Look for internal error text (database errors, file paths) sent to clients. Also look for failures returned with a 2xx status.

## Blocking in handlers

Handlers are async and run on the Tokio runtime; apply the Tokio profile. Password hashing, synchronous database drivers and file I/O belong in `spawn_blocking`.

## Routing (0.8 changes)

- Path syntax is `/{id}` and `/{*rest}`. The 0.7 syntax (`/:id`, `/*rest`) **panics at startup** unless `without_v07_checks()` is used.
- Overlapping routes panic. Static segments take precedence over captures. `nest` strips the prefix.
- Route changes are API changes: removed routes or changed path parameters break clients.
