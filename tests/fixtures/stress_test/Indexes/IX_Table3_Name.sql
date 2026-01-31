CREATE NONCLUSTERED INDEX [IX_Table3_Name]
ON [dbo].[Table3] ([Name])
INCLUDE ([Description], [IsActive]);
GO
