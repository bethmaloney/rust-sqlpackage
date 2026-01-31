CREATE NONCLUSTERED INDEX [IX_Table9_Name]
ON [dbo].[Table9] ([Name])
INCLUDE ([Description], [IsActive]);
GO
