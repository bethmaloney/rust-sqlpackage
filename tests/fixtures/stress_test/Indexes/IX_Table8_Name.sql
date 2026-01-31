CREATE NONCLUSTERED INDEX [IX_Table8_Name]
ON [dbo].[Table8] ([Name])
INCLUDE ([Description], [IsActive]);
GO
