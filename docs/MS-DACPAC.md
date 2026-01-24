# MS-DACPAC: Data-Tier Application Schema File Format

Reference extracted from Microsoft Open Specifications [MS-DACPAC] v20170816.

## Overview

A DAC (Data-Tier Application) package (`.dacpac` file) is a self-contained unit for developing, deploying, and managing data-tier objects. It consists of XML parts that represent metadata and SQL Server object schemas.

### XML Namespaces

```
http://schemas.microsoft.com/sqlserver/ManagementModel/Serialization/yyyy/mm
http://schemas.microsoft.com/sqlserver/RelationalEngine/Serialization/yyyy/mm
```

Version dates: `2009/08`, `2010/11`, `2011/03`

### Document Structure

- Root element: `MM:Instances`
- MIME type: `text/xml`
- File extension: `.xml`

## Structures

### Management Model (MM)

The logical structure definition of a data-tier application instance.

#### MM:Instances (Root Element)

Contains all database object elements. Subelements can appear in any order.

**Subelements:**
- CheckConstraint
- Column
- Database
- DatabaseRole
- DefaultConstraint
- DmlTrigger
- ForeignKeyColumn
- ForeignKeyConstraint
- IndexedColumn
- Login
- PrimaryKeyConstraint
- RelationalIndex
- ScalarParameter
- ScalarValuedFunction
- Schema
- SpatialIndex (2011/03+)
- Statistics (2011/03+)
- StoredProcedure
- Synonym (2011/03+)
- Table
- TableParameter
- TableValuedFunction
- UniqueConstraint
- User
- UserDefinedDataType
- UserDefinedTableType
- View

#### MM:Key Attribute

Unique identifier for an RE element instance. Format: `/Type[Name]/Type[Name]/...`

Example: `/Database[pubs]/Schema[dbo]/Table[employee]`

```xml
<xs:attribute name="Key" type="MM:KeyPatternType" />
<xs:simpleType name="KeyPatternType">
  <xs:restriction base="xs:string">
    <xs:pattern value="(\/.*\[.*\])*" />
  </xs:restriction>
</xs:simpleType>
```

#### MM:ReferenceKey Attribute

References another element by its Key.

```xml
<xs:attribute name="ReferenceKey" type="MM:KeyPatternType" />
```

#### MM:Reference Element

Specifies a referential relationship between RE elements.

```xml
<xs:element name="Reference" type="MM:ReferenceType" />
<xs:complexType name="ReferenceType">
  <xs:attribute ref="MM:ReferenceKey" use="required" />
</xs:complexType>
```

#### MM:ReferencesType

Collection of multiple references.

```xml
<xs:complexType name="ReferencesType">
  <xs:sequence>
    <xs:element minOccurs="1" maxOccurs="unbounded" ref="MM:Reference" />
  </xs:sequence>
</xs:complexType>
```

---

## Relational Engine (RE) Elements

### RE:Database

```xml
<xs:element name="Database">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Collation" type="RE:CollationType" />
        <xs:element name="CompatibilityLevel" type="RE:CompatibilityLevelEnumeration" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:Schema

```xml
<xs:element name="Schema">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Owner" type="MM:ReferenceType" minOccurs="0" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:Table

```xml
<xs:element name="Table">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Columns" type="MM:ReferencesType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:Column

```xml
<xs:element name="Column">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Name" type="xs:string" />
        <xs:element name="DataType" type="RE:DataType" />
        <xs:element name="Nullable" type="RE:BooleanType" />
        <xs:element name="IsColumnSet" type="RE:BooleanType" />
        <xs:element name="IsSparse" type="RE:BooleanType" />
        <xs:element name="RowGuidCol" type="RE:BooleanType" />
        <xs:element name="Collation" type="RE:CollationType" minOccurs="0" />
        <xs:element name="ComputedColumnInfo" type="RE:ComputedColumnType" minOccurs="0" />
        <xs:element name="IdentityColumnInfo" type="RE:IdentityType" minOccurs="0"/>
        <xs:element name="DefaultValue" type="MM:ReferenceType" minOccurs="0"/>
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:View

```xml
<xs:element name="View">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="QueryText" type="xs:string" />
        <xs:element name="Columns" type="MM:ReferencesType" />
        <xs:element name="HasCheckOption" type="RE:BooleanType" />
        <xs:element name="HasColumnSpecification" type="RE:BooleanType" />
        <xs:element name="IsEncrypted" type="RE:BooleanType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
        <xs:element name="IsSchemaBound" type="RE:BooleanType" />
        <xs:element name="ReturnsViewMetadata" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:PrimaryKeyConstraint

```xml
<xs:element name="PrimaryKeyConstraint">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="AssociatedIndex" type="MM:ReferenceType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:UniqueConstraint

```xml
<xs:element name="UniqueConstraint">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="AssociatedIndex" type="MM:ReferenceType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:ForeignKeyConstraint

```xml
<xs:element name="ForeignKeyConstraint">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Columns" type="MM:ReferencesType" />
        <xs:element name="ReferencedTable" type="MM:ReferenceType" />
        <xs:element name="IsChecked" type="RE:BooleanType" />
        <xs:element name="IsEnabled" type="RE:BooleanType" />
        <xs:element name="NotForReplication" type="RE:BooleanType" />
        <xs:element name="DeleteAction" type="RE:DMLActionEnumeration" />
        <xs:element name="UpdateAction" type="RE:DMLActionEnumeration" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:ForeignKeyColumn

```xml
<xs:element name="ForeignKeyColumn">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="ReferencedColumn" type="MM:ReferenceType" />
        <xs:element name="ReferencingColumn" type="MM:ReferenceType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:CheckConstraint

```xml
<xs:element name="CheckConstraint">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Text" type="xs:string" />
        <xs:element name="IsChecked" type="RE:BooleanType" />
        <xs:element name="IsEnabled" type="RE:BooleanType" />
        <xs:element name="NotForReplication" type="RE:BooleanType" /> <!-- 2010/11+ -->
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:DefaultConstraint

```xml
<xs:element name="DefaultConstraint">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Text" type="xs:string" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:RelationalIndex

```xml
<!-- Version 2011/03 -->
<xs:element name="RelationalIndex">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="IndexedColumns" type="MM:ReferencesType" />
        <xs:element name="CompactLargeObjects" type="RE:BooleanType" />
        <xs:element name="DisallowPageLocks" type="RE:BooleanType" />
        <xs:element name="DisallowRowLocks" type="RE:BooleanType" />
        <xs:element name="FillFactor" type="RE:FillFactorType" />
        <xs:element name="FilterDefinition" type="xs:string" />
        <xs:element name="IgnoreDuplicateKeys" type="RE:BooleanType" />
        <xs:element name="IndexKey" type="MM:ReferenceType" minOccurs="0" />
        <xs:element name="IsClustered" type="RE:BooleanType" />
        <xs:element name="IsDisabled" type="RE:BooleanType" />
        <xs:element name="IsUnique" type="RE:BooleanType" />
        <xs:element name="NoAutomaticRecomputation" type="RE:BooleanType" />
        <xs:element name="PadIndex" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:IndexedColumn

```xml
<xs:element name="IndexedColumn">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="ReferencedColumn" type="MM:ReferenceType" />
        <xs:element name="SortOrder" type="RE:SortOrderEnumeration" />
        <xs:element name="IsIncluded" type="xs:string" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:StoredProcedure

```xml
<xs:element name="StoredProcedure">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="BodyText" type="xs:string" />
        <xs:element name="Parameters" type="MM:ReferencesType" minOccurs="0" />
        <xs:element name="ExecutionContext" type="RE:ExecutionContextType" minOccurs="0" />
        <xs:element name="ForReplication" type="RE:BooleanType" />
        <xs:element name="IsEncrypted" type="RE:BooleanType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
        <xs:element name="IsRecompiled" type="RE:BooleanType" />
        <xs:element name="IsSqlClr" type="RE:BooleanType" />
        <xs:element name="Startup" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:ScalarValuedFunction

```xml
<xs:element name="ScalarValuedFunction">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="BodyText" type="xs:string" />
        <xs:element name="DataType" type="RE:DataType" />
        <xs:element name="Parameters" type="MM:ReferencesType" minOccurs="0" />
        <xs:element name="ExecutionContext" type="RE:ExecutionContextType" minOccurs="0" />
        <xs:element name="IsEncrypted" type="RE:BooleanType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
        <xs:element name="IsSchemaBound" type="RE:BooleanType" />
        <xs:element name="IsSqlClr" type="RE:BooleanType" />
        <xs:element name="Nullable" type="RE:BooleanType" />
        <xs:element name="ReturnsNullOnNullInput" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:TableValuedFunction

```xml
<xs:element name="TableValuedFunction">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="BodyText" type="xs:string" />
        <xs:element name="Columns" type="MM:ReferencesType" />
        <xs:element name="Parameters" type="MM:ReferencesType" minOccurs="0" />
        <xs:element name="ExecutionContext" type="RE:ExecutionContextType" minOccurs="0" />
        <xs:element name="IsEncrypted" type="RE:BooleanType" />
        <xs:element name="IsInline" type="RE:BooleanType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
        <xs:element name="IsSchemaBound" type="RE:BooleanType" />
        <xs:element name="IsSqlClr" type="RE:BooleanType" />
        <xs:element name="TableVariableName" type="xs:string" minOccurs="0" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:ScalarParameter

```xml
<xs:element name="ScalarParameter">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Name" type="xs:string" />
        <xs:element name="DataType" type="RE:DataType" />
        <xs:element name="IsOutput" type="RE:BooleanType" />
        <xs:element name="Nullable" type="RE:BooleanType" />
        <xs:element name="DefaultValue" type="xs:string" minOccurs="0" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:DmlTrigger (2011/03)

```xml
<xs:element name="DmlTrigger">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="BodyText" type="xs:string" />
        <xs:element name="InsteadOf" type="RE:BooleanType" />
        <xs:element name="IsEnabled" type="RE:BooleanType" />
        <xs:element name="IsEncrypted" type="RE:BooleanType" />
        <xs:element name="IsQuotedIdentifierOn" type="RE:BooleanType" />
        <xs:element name="NotForReplication" type="RE:BooleanType" />
        <xs:element name="Delete" type="RE:BooleanType" />
        <xs:element name="DeleteActivationOrder" type="RE:ActivationOrder" />
        <xs:element name="Insert" type="RE:BooleanType" />
        <xs:element name="InsertActivationOrder" type="RE:ActivationOrder" />
        <xs:element name="Update" type="RE:BooleanType" />
        <xs:element name="UpdateActivationOrder" type="RE:ActivationOrder" />
        <xs:element name="ExecutionContext" type="RE:ExecutionContextType" minOccurs="0" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:Synonym (2011/03+)

```xml
<xs:element name="Synonym">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="BaseObjectName" type="xs:string" />
        <xs:element name="Name" type="xs:string" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:UserDefinedDataType

```xml
<xs:element name="UserDefinedDataType">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="BaseSystemDataType" type="RE:BaseSystemDataType" />
        <xs:element minOccurs="0" name="Nullable" type="RE:BooleanType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:UserDefinedTableType

```xml
<xs:element name="UserDefinedTableType">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Columns" type="MM:ReferencesType" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:User

```xml
<xs:element name="User">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="UserType" type="RE:UserTypeEnumeration" />
        <xs:element name="Login" type="MM:ReferenceType" minOccurs="0" />
        <xs:element name="MemberOfRoles" type="MM:ReferenceType" minOccurs="0" />
        <xs:element name="DefaultSchema" type="MM:ReferenceType" minOccurs="0" />
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:DatabaseRole

```xml
<xs:element name="DatabaseRole">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Parent" type="MM:ReferenceType" />
        <xs:element name="Name" type="xs:string" />
        <xs:element name="Owner" type="MM:ReferenceType" minOccurs="0" />
        <xs:element name="Permissions" type="RE:Permissions" minOccurs="0" /> <!-- 2011/03+ -->
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

### RE:Login

```xml
<xs:element name="Login">
  <xs:complexType>
    <xs:extension base="MM:InstanceType">
      <xs:all>
        <xs:element name="Name" type="xs:string" />
        <xs:element name="LoginType" type="RE:LoginTypeEnumeration" />
        <xs:element name="Language" type="xs:string" minOccurs="0"/>
      </xs:all>
    </xs:extension>
  </xs:complexType>
</xs:element>
```

---

## Data Types

### RE:DataType

```xml
<xs:complexType name="DataType">
  <xs:choice minOccurs="1" maxOccurs="1">
    <xs:element name="SystemDataType" type="RE:SqlDataType" />
    <xs:element name="XmlDataType" type="RE:SqlDataType" />
    <xs:element name="SystemCLRDataType" type="RE:SqlDataType" /> <!-- 2011/03+ -->
    <xs:element name="ScalarDataType" type="RE:ScalarDataType" />
  </xs:choice>
  <xs:attribute ref="MM:ReferenceKey" use="optional" />
</xs:complexType>
```

### RE:SqlDataType

```xml
<xs:complexType name="SqlDataType">
  <xs:all>
    <xs:element name="Length" type="xs:unsignedByte" />
    <xs:element name="NumericPrecision" type="xs:unsignedByte" />
    <xs:element name="NumericScale" type="xs:unsignedByte" />
    <xs:element name="TypeSpec" type="xs:string" />
  </xs:all>
</xs:complexType>
```

### RE:ScalarDataType

```xml
<xs:complexType name="ScalarDataType">
  <xs:all>
    <xs:element name="Name" type="xs:string"/>
  </xs:all>
</xs:complexType>
```

### RE:BaseSystemDataType

```xml
<xs:complexType name="BaseSystemDataType">
  <xs:all>
    <xs:element name="SystemDataType" type="RE:SqlDataType" />
  </xs:all>
</xs:complexType>
```

### RE:ComputedColumnType

```xml
<xs:complexType name="ComputedColumnType">
  <xs:all>
    <xs:element name="Text" type="xs:string" />
    <xs:element name="IsPersisted" type="RE:BooleanType" />
  </xs:all>
</xs:complexType>
```

### RE:IdentityType

```xml
<xs:complexType name="IdentityType">
  <xs:sequence>
    <xs:element name="Seed" type="xs:unsignedInt" />
    <xs:element name="Increment" type="xs:unsignedInt" />
    <xs:element name="NotForReplication" type="RE:BooleanType" /> <!-- 2010/11+ -->
  </xs:sequence>
</xs:complexType>
```

### RE:CollationType

```xml
<xs:complexType name="CollationType">
  <xs:all>
    <xs:element name="Name" type="RE:CollationEnumeration" />
  </xs:all>
</xs:complexType>
```

---

## Enumerations

### RE:BooleanType

```xml
<xs:simpleType name="BooleanType">
  <xs:restriction base="xs:string">
    <xs:enumeration value="True" />
    <xs:enumeration value="False" />
  </xs:restriction>
</xs:simpleType>
```

### RE:SortOrderEnumeration

```xml
<xs:simpleType name="SortOrderEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="Ascending" />
    <xs:enumeration value="Descending" />
  </xs:restriction>
</xs:simpleType>
```

### RE:DMLActionEnumeration

```xml
<xs:simpleType name="DMLActionEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="NoAction" />
    <xs:enumeration value="Cascade" />
    <xs:enumeration value="SetNull" />
    <xs:enumeration value="SetDefault" />
  </xs:restriction>
</xs:simpleType>
```

### RE:CompatibilityLevelEnumeration

```xml
<xs:simpleType name="CompatibilityLevelEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="Version80" />
    <xs:enumeration value="Version90" />
    <xs:enumeration value="Version100" />
    <xs:enumeration value="Version110" /> <!-- 2011/03+ -->
    <xs:enumeration value="Current" />
  </xs:restriction>
</xs:simpleType>
```

### RE:ExecuteAsEnumeration

```xml
<xs:simpleType name="ExecuteAsEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="Caller" />
    <xs:enumeration value="Self" />
    <xs:enumeration value="Owner" />
    <xs:enumeration value="ExecuteAsUser" />
    <xs:enumeration value="ExecuteAsLogin" /> <!-- 2011/03+ -->
  </xs:restriction>
</xs:simpleType>
```

### RE:ActivationOrder (2011/03+)

```xml
<xs:simpleType name="ActivationOrder">
  <xs:restriction base="xs:string">
    <xs:enumeration value="None" />
    <xs:enumeration value="First" />
    <xs:enumeration value="Last" />
  </xs:restriction>
</xs:simpleType>
```

### RE:UserTypeEnumeration

```xml
<xs:simpleType name="UserTypeEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="NoLogin" />
    <xs:enumeration value="SqlLogin" />
  </xs:restriction>
</xs:simpleType>
```

### RE:LoginTypeEnumeration

```xml
<xs:simpleType name="LoginTypeEnumeration">
  <xs:restriction base="xs:string">
    <xs:enumeration value="Sql" />
    <xs:enumeration value="Windows" />
  </xs:restriction>
</xs:simpleType>
```

### RE:FillFactorType

```xml
<xs:simpleType name="FillFactorType">
  <xs:restriction base="xs:short">
    <xs:minInclusive value="0"/>
    <xs:maxInclusive value="100"/>
  </xs:restriction>
</xs:simpleType>
```

---

## Structure Examples

### Logical Object Sample (model.xml)

```xml
<?xml version="1.0" encoding="utf-8"?>
<MM:Instances
xmlns:MM="http://schemas.microsoft.com/sqlserver/ManagementModel/Serialization/2009/08"
xmlns:RE="http://schemas.microsoft.com/sqlserver/RelationalEngine/Serialization/2009/08">

    <!-- Database -->
    <RE:Database MM:Key="/Database[pubs]">
        <RE:Collation>
            <RE:Name>SQL_Latin1_General_CP1_CI_AS</RE:Name>
        </RE:Collation>
        <RE:CompatibilityLevel>Version100</RE:CompatibilityLevel>
        <RE:Name>pubs</RE:Name>
    </RE:Database>

    <!-- Schema -->
    <RE:Schema MM:Key="/Database[pubs]/Schema[dbo]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]" />
        <RE:Name>dbo</RE:Name>
        <RE:Owner MM:ReferenceKey="/Database[pubs]/User[dbo]" />
    </RE:Schema>

    <!-- Table -->
    <RE:Table MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]/Schema[dbo]" />
        <RE:Columns>
            <MM:Reference MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/Column[emp_id]" />
            <MM:Reference MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/Column[fname]" />
        </RE:Columns>
        <RE:IsQuotedIdentifierOn>True</RE:IsQuotedIdentifierOn>
        <RE:Name>employee</RE:Name>
    </RE:Table>

    <!-- Column with User-Defined Type Reference -->
    <RE:Column MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/Column[emp_id]">
        <RE:Collation>
            <RE:Name>SQL_Latin1_General_CP1_CI_AS</RE:Name>
        </RE:Collation>
        <RE:DataType MM:ReferenceKey="/Database[pubs]/Schema[dbo]/UserDefinedDataType[empid]" />
        <RE:IsColumnSet>False</RE:IsColumnSet>
        <RE:IsSparse>False</RE:IsSparse>
        <RE:Name>emp_id</RE:Name>
        <RE:Nullable>False</RE:Nullable>
        <RE:RowGuidCol>False</RE:RowGuidCol>
    </RE:Column>

    <!-- Column with System Data Type -->
    <RE:Column MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/Column[fname]">
        <RE:Collation>
            <RE:Name>SQL_Latin1_General_CP1_CI_AS</RE:Name>
        </RE:Collation>
        <RE:DataType>
            <RE:SystemDataType>
                <RE:Length>20</RE:Length>
                <RE:NumericPrecision>0</RE:NumericPrecision>
                <RE:NumericScale>0</RE:NumericScale>
                <RE:TypeSpec>VarChar</RE:TypeSpec>
            </RE:SystemDataType>
        </RE:DataType>
        <RE:IsColumnSet>False</RE:IsColumnSet>
        <RE:IsSparse>False</RE:IsSparse>
        <RE:Name>fname</RE:Name>
        <RE:Nullable>False</RE:Nullable>
        <RE:RowGuidCol>False</RE:RowGuidCol>
    </RE:Column>

    <!-- Check Constraint -->
    <RE:CheckConstraint MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/CheckConstraint[CK_emp_id]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]" />
        <RE:IsChecked>True</RE:IsChecked>
        <RE:IsEnabled>True</RE:IsEnabled>
        <RE:Name>CK_emp_id</RE:Name>
        <RE:Text>([emp_id] like '[A-Z][A-Z][A-Z][1-9][0-9][0-9][0-9][0-9][FM]')</RE:Text>
    </RE:CheckConstraint>

    <!-- Primary Key Constraint -->
    <RE:PrimaryKeyConstraint MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/PrimaryKeyConstraint[PK_emp_id]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]" />
        <RE:AssociatedIndex MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[PK_emp_id]" />
        <RE:Name>PK_emp_id</RE:Name>
    </RE:PrimaryKeyConstraint>

    <!-- Default Constraint -->
    <RE:DefaultConstraint MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/Column[job_id]/DefaultConstraint[DF_job_id]">
        <RE:Name>DF_job_id</RE:Name>
        <RE:Text>((1))</RE:Text>
    </RE:DefaultConstraint>

</MM:Instances>
```

### Physical Object Sample (Indexes)

```xml
<?xml version="1.0" encoding="utf-8"?>
<MM:Instances
xmlns:MM="http://schemas.microsoft.com/sqlserver/ManagementModel/Serialization/2009/08"
xmlns:RE="http://schemas.microsoft.com/sqlserver/RelationalEngine/Serialization/2009/08">

    <!-- Clustered Index -->
    <RE:RelationalIndex MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[employee_ind]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]" />
        <RE:CompactLargeObjects>True</RE:CompactLargeObjects>
        <RE:DisallowPageLocks>False</RE:DisallowPageLocks>
        <RE:DisallowRowLocks>False</RE:DisallowRowLocks>
        <RE:FillFactor>0</RE:FillFactor>
        <RE:FilterDefinition></RE:FilterDefinition>
        <RE:IgnoreDuplicateKeys>False</RE:IgnoreDuplicateKeys>
        <RE:IndexedColumns>
            <MM:Reference MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[employee_ind]/IndexedColumn[lname]" />
            <MM:Reference MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[employee_ind]/IndexedColumn[fname]" />
        </RE:IndexedColumns>
        <RE:IsClustered>True</RE:IsClustered>
        <RE:IsDisabled>False</RE:IsDisabled>
        <RE:IsUnique>False</RE:IsUnique>
        <RE:MaximumDegreeOfParallelism>-1</RE:MaximumDegreeOfParallelism>
        <RE:Name>employee_ind</RE:Name>
        <RE:NoAutomaticRecomputation>False</RE:NoAutomaticRecomputation>
        <RE:OnlineIndexOperation>False</RE:OnlineIndexOperation>
        <RE:PadIndex>False</RE:PadIndex>
        <RE:SortInTempdb>False</RE:SortInTempdb>
    </RE:RelationalIndex>

    <!-- Primary Key Index -->
    <RE:RelationalIndex MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[PK_emp_id]">
        <RE:Parent MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]" />
        <RE:CompactLargeObjects>True</RE:CompactLargeObjects>
        <RE:DisallowPageLocks>False</RE:DisallowPageLocks>
        <RE:DisallowRowLocks>False</RE:DisallowRowLocks>
        <RE:FillFactor>0</RE:FillFactor>
        <RE:FilterDefinition></RE:FilterDefinition>
        <RE:IgnoreDuplicateKeys>False</RE:IgnoreDuplicateKeys>
        <RE:IndexKey MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/PrimaryKeyConstraint[PK_emp_id]" />
        <RE:IndexedColumns>
            <MM:Reference MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[PK_emp_id]/IndexedColumn[emp_id]" />
        </RE:IndexedColumns>
        <RE:IsClustered>False</RE:IsClustered>
        <RE:IsDisabled>False</RE:IsDisabled>
        <RE:IsUnique>True</RE:IsUnique>
        <RE:MaximumDegreeOfParallelism>-1</RE:MaximumDegreeOfParallelism>
        <RE:Name>PK_emp_id</RE:Name>
        <RE:NoAutomaticRecomputation>False</RE:NoAutomaticRecomputation>
        <RE:OnlineIndexOperation>False</RE:OnlineIndexOperation>
        <RE:PadIndex>False</RE:PadIndex>
        <RE:SortInTempdb>False</RE:SortInTempdb>
    </RE:RelationalIndex>

    <!-- Indexed Column -->
    <RE:IndexedColumn MM:Key="/Database[pubs]/Schema[dbo]/Table[employee]/RelationalIndex[employee_ind]/IndexedColumn[lname]">
        <RE:IsIncluded>False</RE:IsIncluded>
        <RE:ReferencedColumn MM:ReferenceKey="/Database[pubs]/Schema[dbo]/Table[employee]/Column[lname]" />
        <RE:SortOrder>Ascending</RE:SortOrder>
    </RE:IndexedColumn>

</MM:Instances>
```

---

## Key Pattern Examples

| Object Type | Key Pattern |
|-------------|-------------|
| Database | `/Database[DbName]` |
| Schema | `/Database[DbName]/Schema[SchemaName]` |
| Table | `/Database[DbName]/Schema[SchemaName]/Table[TableName]` |
| Column | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/Column[ColumnName]` |
| View | `/Database[DbName]/Schema[SchemaName]/View[ViewName]` |
| Index | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/RelationalIndex[IndexName]` |
| IndexedColumn | `.../RelationalIndex[IndexName]/IndexedColumn[ColumnName]` |
| PrimaryKey | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/PrimaryKeyConstraint[PKName]` |
| ForeignKey | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/ForeignKeyConstraint[FKName]` |
| UniqueConstraint | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/UniqueConstraint[UQName]` |
| CheckConstraint | `/Database[DbName]/Schema[SchemaName]/Table[TableName]/CheckConstraint[CKName]` |
| DefaultConstraint | `.../Column[ColumnName]/DefaultConstraint[DFName]` |
| StoredProcedure | `/Database[DbName]/Schema[SchemaName]/StoredProcedure[ProcName]` |
| Function | `/Database[DbName]/Schema[SchemaName]/ScalarValuedFunction[FuncName]` |
| User | `/Database[DbName]/User[UserName]` |

---

## TypeSpec Values for SqlDataType

Common values for the `TypeSpec` element:

- `BigInt`, `Int`, `SmallInt`, `TinyInt`
- `Bit`
- `Decimal`, `Numeric`, `Money`, `SmallMoney`
- `Float`, `Real`
- `Date`, `DateTime`, `DateTime2`, `DateTimeOffset`, `SmallDateTime`, `Time`
- `Char`, `VarChar`, `Text`
- `NChar`, `NVarChar`, `NText`
- `Binary`, `VarBinary`, `Image`
- `UniqueIdentifier`
- `Xml`
- `Geography`, `Geometry`
- `HierarchyId`
- `Sql_Variant`

---

## Version History

| Version | Release | Notes |
|---------|---------|-------|
| 2009/08 | SQL Server 2008 R2 | Initial release |
| 2010/11 | SQL Server 2008 R2 DAC OOB | Added NotForReplication to IdentityType, CheckConstraint |
| 2011/03 | SQL Server 2012 | Added SpatialIndex, Statistics, Synonym, ActivationOrder, SystemCLRDataType |

---

*Source: [MS-DACPAC] v20170816 - Microsoft Open Specifications*
