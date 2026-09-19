# Serde

Apply this only when serde is in use. Serialisation formats are contracts with stored data and other services. Changes break silently: the data still parses, but into the wrong thing.

## Look for

- **Representation changes that break existing data:**
  - renamed fields without `#[serde(alias = "...")]`;
  - changed field types;
  - a changed enum tagging (`tag`, `content`, `untagged`);
  - `rename_all` changes;
  - reordered tuple fields.
- **`#[serde(untagged)]` ambiguity.** Serde tries variants in order and takes the first that deserialises. Serde ignores unknown fields by default, and `Option` fields may be absent. An earlier variant therefore often matches data meant for a later one. Refunds become payments.
- **`#[serde(default)]` masking invalid input.** A missing required field silently becomes zero or empty. Consider whether "missing" should be an error.
- **Optionality.** `Option<T>` without `skip_serializing_if` writes `null`, which other consumers may reject. A field that became required breaks old data.
- **Silent data loss.** Without `#[serde(deny_unknown_fields)]`, typos in config keys are ignored. That is sometimes intended and sometimes a bug.
- **Untrusted input.** Deserialising untrusted data into unbounded collections without a size limit. Pair with the security reference.

## Do not flag

Serde attributes on types that are never persisted or sent over the wire (check usage with Grep).

## Evidence that makes it a finding

A concrete payload and what it deserialises to. For example, `{"order_id":"a","amount":5,"reason":"dup"}` now becomes `Event::Payment`.
