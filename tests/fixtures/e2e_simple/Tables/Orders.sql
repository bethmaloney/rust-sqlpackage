CREATE TABLE [Sales].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATETIME NOT NULL,
    [TotalAmount] DECIMAL(18, 2) NOT NULL,
    CONSTRAINT [FK_Orders_Customers] FOREIGN KEY ([CustomerId]) REFERENCES [Sales].[Customers]([Id])
);
GO
