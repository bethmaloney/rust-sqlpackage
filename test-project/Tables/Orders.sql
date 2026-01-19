CREATE TABLE [dbo].[Orders] (
    [Id] INT NOT NULL PRIMARY KEY,
    [UserId] INT NOT NULL,
    [OrderDate] DATETIME2 NOT NULL,
    [TotalAmount] DECIMAL(18, 2) NOT NULL,
    CONSTRAINT [FK_Orders_Users] FOREIGN KEY ([UserId]) REFERENCES [dbo].[Users]([Id])
);

CREATE INDEX [IX_Orders_UserId] ON [dbo].[Orders] ([UserId]);
