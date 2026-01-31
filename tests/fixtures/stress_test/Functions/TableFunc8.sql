CREATE FUNCTION [dbo].[TableFunc8]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table9]
    WHERE [IsActive] = @IsActive
);
GO
