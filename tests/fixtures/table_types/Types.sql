-- User-defined table type with columns and constraints
CREATE TYPE [dbo].[OrderItemsType] AS TABLE (
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18, 2) NOT NULL,
    [Discount] DECIMAL(5, 2) NOT NULL DEFAULT 0,
    PRIMARY KEY CLUSTERED ([ProductId])
);
GO

-- Table type with multiple columns
CREATE TYPE [dbo].[IdListType] AS TABLE (
    [Id] INT NOT NULL,
    [SortOrder] INT NOT NULL DEFAULT 0,
    INDEX [IX_IdList_SortOrder] NONCLUSTERED ([SortOrder])
);
GO

-- Table type with check constraint
CREATE TYPE [dbo].[PercentageType] AS TABLE (
    [Name] NVARCHAR(50) NOT NULL,
    [Value] DECIMAL(5, 2) NOT NULL,
    CHECK ([Value] >= 0 AND [Value] <= 100)
);
GO
