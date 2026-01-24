-- Filtered index with status filter (equality)
CREATE NONCLUSTERED INDEX [IX_Orders_Recent]
ON [dbo].[Orders] ([OrderDate] DESC)
WHERE [Status] = N'Pending' AND [IsActive] = 1;
GO
