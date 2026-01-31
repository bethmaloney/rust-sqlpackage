CREATE FUNCTION [dbo].[TableFunc9]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table10]
    WHERE [IsActive] = @IsActive
);
GO
