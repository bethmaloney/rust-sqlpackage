-- Table with various indexes to test naming
CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATE NOT NULL,
    [Status] NVARCHAR(50) NOT NULL,
    [TotalAmount] DECIMAL(18, 2) NOT NULL
);
GO

-- Index with standard naming
CREATE NONCLUSTERED INDEX [IX_Orders_CustomerId]
ON [dbo].[Orders] ([CustomerId]);
GO

-- Index with multiple columns
CREATE NONCLUSTERED INDEX [IX_Orders_Status_OrderDate]
ON [dbo].[Orders] ([Status], [OrderDate] DESC);
GO

-- Unique index
CREATE UNIQUE NONCLUSTERED INDEX [UX_Orders_CustomerDate]
ON [dbo].[Orders] ([CustomerId], [OrderDate]);
GO

-- Index with INCLUDE
CREATE NONCLUSTERED INDEX [IX_Orders_Status_Include]
ON [dbo].[Orders] ([Status])
INCLUDE ([TotalAmount], [OrderDate]);
GO

-- Another table to test cross-table index naming
CREATE TABLE [dbo].[OrderItems] (
    [Id] INT NOT NULL PRIMARY KEY,
    [OrderId] INT NOT NULL,
    [ProductId] INT NOT NULL,
    [Quantity] INT NOT NULL
);
GO

CREATE NONCLUSTERED INDEX [IX_OrderItems_OrderId]
ON [dbo].[OrderItems] ([OrderId]);
GO

CREATE NONCLUSTERED INDEX [IX_OrderItems_ProductId]
ON [dbo].[OrderItems] ([ProductId]);
GO
