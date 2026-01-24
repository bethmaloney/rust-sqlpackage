-- Table with computed columns (non-persisted)
CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18,2) NOT NULL,
    [DiscountPercent] DECIMAL(5,2) NOT NULL DEFAULT 0,

    -- Non-persisted computed columns
    [TotalValue] AS ([Quantity] * [UnitPrice]),
    [DiscountedPrice] AS ([UnitPrice] * (1 - [DiscountPercent] / 100)),
    [DisplayName] AS (UPPER([Name])),

    CONSTRAINT [PK_Products] PRIMARY KEY ([Id])
);
GO
