CREATE NONCLUSTERED INDEX [IX_Table10_Name]
ON [dbo].[Table10] ([Name])
INCLUDE ([Description], [IsActive]);
GO
