CREATE FUNCTION [dbo].[TableFunc10]
(
    @IsActive BIT = 1
)
RETURNS TABLE
AS
RETURN
(
    SELECT [Id], [Name], [Description], [Amount], [Quantity], [CreatedDate]
    FROM [dbo].[Table11]
    WHERE [IsActive] = @IsActive
);
GO
