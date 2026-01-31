CREATE VIEW [dbo].[View3]
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
FROM [dbo].[Table4]
WHERE [IsActive] = 1;
GO
