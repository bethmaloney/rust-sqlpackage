CREATE NONCLUSTERED INDEX [IX_Table4_Name]
ON [dbo].[Table4] ([Name])
INCLUDE ([Description], [IsActive]);
GO
