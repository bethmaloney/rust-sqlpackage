CREATE NONCLUSTERED INDEX [IX_Table5_Name]
ON [dbo].[Table5] ([Name])
INCLUDE ([Description], [IsActive]);
GO
