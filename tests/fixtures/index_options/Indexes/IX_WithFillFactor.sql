-- Index with FILLFACTOR option (controls page fill percentage)
CREATE NONCLUSTERED INDEX [IX_LargeTable_Category_FillFactor]
ON [dbo].[LargeTable] ([Category])
WITH (FILLFACTOR = 80);
GO
