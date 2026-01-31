CREATE VIEW [dbo].[View2]
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
FROM [dbo].[Table3]
WHERE [IsActive] = 1;
GO
