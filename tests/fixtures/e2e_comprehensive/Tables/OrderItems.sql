CREATE TABLE [Sales].[OrderItems] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [OrderId] INT NOT NULL,
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18, 2) NOT NULL,
    [LineTotal] AS ([Quantity] * [UnitPrice]),

    CONSTRAINT [PK_OrderItems] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [FK_OrderItems_Orders] FOREIGN KEY ([OrderId]) REFERENCES [Sales].[Orders]([Id]),
    CONSTRAINT [FK_OrderItems_Products] FOREIGN KEY ([ProductId]) REFERENCES [dbo].[Products]([Id]),
    CONSTRAINT [CK_OrderItems_Quantity] CHECK ([Quantity] > 0),
    CONSTRAINT [CK_OrderItems_UnitPrice] CHECK ([UnitPrice] >= 0)
);
GO
