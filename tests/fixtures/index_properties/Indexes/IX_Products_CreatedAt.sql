-- Clustered index on CreatedAt
CREATE CLUSTERED INDEX [IX_Products_CreatedAt]
ON [dbo].[Products] ([CreatedAt] DESC);
