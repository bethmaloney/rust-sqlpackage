-- Filtered index on active orders only (soft delete pattern)
CREATE NONCLUSTERED INDEX [IX_Orders_Active]
ON [dbo].[Orders] ([CustomerId], [OrderDate])
WHERE [DeletedAt] IS NULL;
GO
