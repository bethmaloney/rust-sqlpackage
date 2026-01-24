-- Index with DATA_COMPRESSION option (reduces storage)
CREATE NONCLUSTERED INDEX [IX_LargeTable_ModifiedAt_Compressed]
ON [dbo].[LargeTable] ([ModifiedAt])
WITH (DATA_COMPRESSION = PAGE);
GO
