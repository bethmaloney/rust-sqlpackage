# Dacpac Compatibility Plan

## Reference Schema

The official dacpac XML schema is available at:
https://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02/dacpac.xsd

This XSD defines the valid structure for model.xml, Origin.xml, and other dacpac components.

## Current Issues

The generated dacpac fails to deploy with SqlPackage due to XML format incompatibilities:

1. **Annotation format** - `SqlInlineConstraintAnnotation` does not support a `Script` property
2. **Procedure/View/Function bodies** - Should use `BodyScript` property with CDATA, not Annotation with Script

## Required Changes

### 1. Fix Annotation usage
- Annotations should be simple markers: `<Annotation Type="SqlInlineConstraintAnnotation" />`
- They should NOT contain Script properties

### 2. Fix procedure/view/function body storage
- Use `<Property Name="BodyScript"><Value><![CDATA[...]]></Value></Property>` format
- Match the official DacFx output structure

## Testing

### Add integration/e2e test for XSD validation

Add a test that:
1. Generates a dacpac from a test fixture
2. Extracts model.xml from the dacpac
3. Validates model.xml against the official XSD schema
4. Ensures the dacpac can be successfully published by SqlPackage

This will catch format incompatibilities early and ensure ongoing compatibility with Microsoft tooling.

```rust
// Example test structure
#[test]
fn test_model_xml_validates_against_xsd() {
    // 1. Generate dacpac
    // 2. Extract model.xml
    // 3. Validate against XSD
    // 4. Optionally: run sqlpackage /Action:Script to verify it can parse the dacpac
}
```
