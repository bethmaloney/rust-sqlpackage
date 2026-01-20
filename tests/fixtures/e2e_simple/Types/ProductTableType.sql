CREATE TYPE [dbo].[ProductTableType] AS TABLE
(
    [SKU] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [Price] DECIMAL(18, 2) NOT NULL
);
GO
