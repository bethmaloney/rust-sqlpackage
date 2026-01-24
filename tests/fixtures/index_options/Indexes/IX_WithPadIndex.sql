-- Index with PAD_INDEX option (applies fill factor to intermediate pages)
CREATE NONCLUSTERED INDEX [IX_LargeTable_Name_PadIndex]
ON [dbo].[LargeTable] ([Name])
WITH (PAD_INDEX = ON, FILLFACTOR = 70);
GO
