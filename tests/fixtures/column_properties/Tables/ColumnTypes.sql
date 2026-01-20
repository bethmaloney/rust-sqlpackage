CREATE TABLE [dbo].[ColumnTypes] (
    [Id] INT NOT NULL PRIMARY KEY,
    [RequiredName] NVARCHAR(100) NOT NULL,
    [OptionalName] NVARCHAR(100) NULL,
    [Code] VARCHAR(10) NOT NULL,
    [ShortDescription] VARCHAR(50) NULL,
    [CountryCode] CHAR(2) NOT NULL,
    [LongDescription] VARCHAR(MAX) NULL,
    [Notes] NVARCHAR(MAX) NULL,
    [Price] DECIMAL(18, 2) NOT NULL,
    [TaxRate] DECIMAL(5, 4) NULL,
    [SmallData] VARBINARY(100) NULL,
    [Quantity] NUMERIC(10, 0) NOT NULL
);
