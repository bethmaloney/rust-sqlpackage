CREATE NONCLUSTERED INDEX [IX_IndexedTable_Category_Name]
ON [dbo].[IndexedTable] ([Category], [Name])
INCLUDE ([Description]);
