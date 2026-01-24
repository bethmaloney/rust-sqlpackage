-- Table with composite FK referencing 3-column composite PK
CREATE TABLE [dbo].[OrderLines] (
    [Id] INT NOT NULL,
    [OrderYear] INT NOT NULL,
    [OrderRegion] CHAR(2) NOT NULL,
    [OrderNumber] INT NOT NULL,
    [LineNumber] INT NOT NULL,
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL,
    [UnitPrice] DECIMAL(18,2) NOT NULL,
    [LineTotal] AS ([Quantity] * [UnitPrice]),
    CONSTRAINT [PK_OrderLines] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_OrderLines_OrderHeaders] FOREIGN KEY ([OrderYear], [OrderRegion], [OrderNumber])
        REFERENCES [dbo].[OrderHeaders]([Year], [Region], [OrderNumber])
        ON DELETE CASCADE,
    CONSTRAINT [UQ_OrderLines_Order_Line] UNIQUE ([OrderYear], [OrderRegion], [OrderNumber], [LineNumber])
);
GO
