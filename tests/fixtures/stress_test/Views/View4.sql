CREATE VIEW [dbo].[View4]
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
FROM [dbo].[Table5]
WHERE [IsActive] = 1;
GO
