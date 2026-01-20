CREATE TABLE [Inventory].[InventoryLog] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [ProductId] INT NOT NULL,
    [QuantityChange] INT NOT NULL,
    [Reason] NVARCHAR(200) NOT NULL,
    [LoggedAt] DATETIME NOT NULL,

    CONSTRAINT [PK_InventoryLog] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [FK_InventoryLog_Products] FOREIGN KEY ([ProductId]) REFERENCES [dbo].[Products]([Id]),
    CONSTRAINT [DF_InventoryLog_LoggedAt] DEFAULT (GETDATE()) FOR [LoggedAt]
);
GO
