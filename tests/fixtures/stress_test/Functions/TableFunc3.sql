CREATE FUNCTION [dbo].[TableFunc3]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table4]
    WHERE [IsActive] = @IsActive
);
GO
