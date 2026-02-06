# compare_dacpacs.py

Compares a rust-sqlpackage-generated dacpac against a .NET DacFx-generated dacpac, reporting missing elements, extra elements, and differing attribute values. Designed for regression detection as the rust compiler evolves.

## Requirements

Python 3 (stdlib only, no dependencies).

## Usage

```bash
# Compare two .dacpac files
python3 tools/compare_dacpacs.py <rust_dacpac> <dotnet_dacpac>

# Also accepts pre-extracted directories
python3 tools/compare_dacpacs.py /path/to/extracted_rust/ /path/to/extracted_dotnet/
```

The first argument is always the rust-generated dacpac; the second is the dotnet reference.

## Exit codes

- `0` — dacpacs are identical (ignoring Origin.xml)
- `1` — differences found

## What gets compared

| File | Strategy |
|------|----------|
| `Origin.xml` | Skipped (contains build timestamps/GUIDs that always differ) |
| `DacMetadata.xml` | Canonical XML tree comparison (order-independent) |
| `[Content_Types].xml` | Canonical XML tree comparison (order-independent) |
| `predeploy.sql` | Line-by-line text diff (trailing whitespace stripped) |
| `postdeploy.sql` | Line-by-line text diff (trailing whitespace stripped) |
| `model.xml` Header | CustomData entries indexed by `(Category, Type)`, metadata values compared |
| `model.xml` Elements | Semantic element-by-element comparison (see below) |
| Other files | Auto-discovered; files only in one dacpac are flagged, files in both are text-diffed |

## model.xml comparison details

model.xml is the core schema file (~14MB, ~32K elements). The tool compares it semantically rather than textually, so ordering and whitespace differences are ignored.

### Element matching

Each `<Element>` is matched between the two files using a unique key:

- **Named elements** (have a `Name` attribute): matched by `(Type, Name)` — e.g. `(SqlTable, [dbo].[Account])`
- **Unnamed elements** (e.g. inline constraints): matched by `(Type, DefiningTable + ForColumn)` or `(Type, DefiningTable)` composite key
- **Singletons** (e.g. `SqlDatabaseOptions`): matched by `(Type,)` alone

### Per-element diff

For each matched pair, the tool compares:

- **Properties** — by `Name`; compares `Value` attribute or CDATA text content
- **Relationships** — by `Name`; compares `Entry`/`References` children (including `ExternalSource`)
- **Annotations** — compared as a sorted list of `(Type, properties)` tuples, supporting multiple annotations of the same type (`Disambiguator` values are ignored since they are sequential IDs that may differ between builds)

### Inline elements

Inline elements nested inside Relationship entries are compared by fingerprint (a canonical string of their Type, Properties, Relationships, and Annotations). All components are sorted for order-independence.

## Example output

```
=== Dacpac Comparison Report ===

--- DacMetadata.xml ---
OK (identical)

--- [Content_Types].xml ---
OK (identical)

--- predeploy.sql ---
OK (identical)

--- model.xml: Header ---
OK (identical)

--- model.xml: Elements ---
Total elements: rust=8126, dotnet=8126

Missing in rust (1):
  SqlSchema [Datawarehouse]

Extra in rust (0):
  (none)

Differences (1):
  SqlDatabaseOptions:
    Property "IsAnsiNullsOn": dotnet="True", rust="False"

Summary: 1 missing, 0 extra, 1 different
```

## Warnings

Duplicate element keys within a single dacpac are reported to stderr. This would indicate either a bug in the dacpac generator or an element type that needs a more specific keying strategy.
