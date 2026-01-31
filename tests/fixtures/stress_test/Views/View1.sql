CREATE VIEW [dbo].[View1]
AS
SELECT 
    [Id],
    [Name],
    [Description],
    [Amount],
    [Quantity],
    [IsActive],
    [CreatedDate],
    [ModifiedDate],
    [Status],
    [Code]
FROM [dbo].[Table2]
WHERE [IsActive] = 1;
GO
