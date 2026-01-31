CREATE NONCLUSTERED INDEX [IX_Table6_Name]
ON [dbo].[Table6] ([Name])
INCLUDE ([Description], [IsActive]);
GO
