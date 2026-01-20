CREATE TYPE [Sales].[OrderItemTableType] AS TABLE
(
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18, 2) NOT NULL
);
GO
