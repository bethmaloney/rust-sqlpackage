CREATE TABLE [Sales].[Orders] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [OrderNumber] NVARCHAR(50) NOT NULL,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATETIME NOT NULL CONSTRAINT [DF_Orders_OrderDate] DEFAULT (GETDATE()),
    [TotalAmount] DECIMAL(18, 2) NOT NULL,
    [Status] NVARCHAR(20) NOT NULL CONSTRAINT [DF_Orders_Status] DEFAULT ('Pending'),

    CONSTRAINT [PK_Orders] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [FK_Orders_Customers] FOREIGN KEY ([CustomerId]) REFERENCES [Sales].[Customers]([Id]),
    CONSTRAINT [UQ_Orders_OrderNumber] UNIQUE ([OrderNumber]),
    CONSTRAINT [CK_Orders_TotalAmount] CHECK ([TotalAmount] >= 0),
    CONSTRAINT [CK_Orders_Status] CHECK ([Status] IN ('Pending', 'Processing', 'Shipped', 'Delivered', 'Cancelled'))
);
GO
