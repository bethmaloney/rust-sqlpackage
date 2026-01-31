CREATE VIEW [dbo].[View9]
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
FROM [dbo].[Table10]
WHERE [IsActive] = 1;
GO
