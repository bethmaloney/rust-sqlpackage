CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [CustomerId] INT NOT NULL,
    [OrderDate] DATETIME NOT NULL,
    [Status] NVARCHAR(50) NOT NULL,
    [IsArchived] BIT NOT NULL DEFAULT 0
);
GO

-- Standard CREATE INDEX (no CLUSTERED/NONCLUSTERED keyword)
CREATE INDEX [IX_Orders_CustomerId] ON [dbo].[Orders] ([CustomerId]);
GO

CREATE INDEX [IX_Orders_Status_Date] ON [dbo].[Orders] ([Status], [OrderDate] DESC);
GO

-- Filtered index
CREATE INDEX [IX_Orders_Active] ON [dbo].[Orders] ([Status])
WHERE [IsArchived] = 0;
GO

-- Index with INCLUDE
CREATE INDEX [IX_Orders_Customer_Include] ON [dbo].[Orders] ([CustomerId])
INCLUDE ([OrderDate], [Status]);
GO
