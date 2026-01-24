CREATE TABLE [dbo].[ProductHistory] (
    [HistoryId] INT NOT NULL IDENTITY(1,1),
    [ProductId] INT NOT NULL,
    [Action] VARCHAR(10) NOT NULL,
    [OldName] NVARCHAR(100) NULL,
    [NewName] NVARCHAR(100) NULL,
    [OldPrice] DECIMAL(18,2) NULL,
    [NewPrice] DECIMAL(18,2) NULL,
    [ChangedAt] DATETIME NOT NULL DEFAULT GETDATE(),
    CONSTRAINT [PK_ProductHistory] PRIMARY KEY ([HistoryId])
);
GO
