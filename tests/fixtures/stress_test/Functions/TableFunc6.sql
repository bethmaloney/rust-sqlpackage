CREATE FUNCTION [dbo].[TableFunc6]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table7]
    WHERE [IsActive] = @IsActive
);
GO
